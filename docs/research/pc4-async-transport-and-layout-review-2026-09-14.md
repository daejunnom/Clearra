# PC4 비동기 탐색 큐·HTTP 전송·파일 배치 검토

작성: 2026-09-14. 소스 기준: `6172f58eaa6c8e0e7cbde902e88a1077ec242d9f`,
`codex/v0.9.0-stacked-on-v0.8.1-20260912` 작업 트리.

이 문서의 1~7절은 2026-09-14 당시 사용자 제안과 소스/실제 전송을 비교한 설계 후보다.
진행 중이던 compact union의 graph 연결 작업은 비교를 위해 일시 정지했다.
기존 작성 중인 Rust helper를 완료·검증된 제품 구현으로 취급하지 않는다.
4194 교체, 새 전체 탐색, 빌드, 배포 또는 프로필 활성화는 수행하지 않았다.

## 1. 결론

CPU 작업이 I/O 응답을 기다리지 않고 다른 준비된 작업으로 넘어가는 방향이 맞다.
다만 스레드마다 독립 HTTP 클라이언트를 만들고 요청을 무제한 발사하는 형태가 아니라,
**준비된 계산 작업 큐 + 응답 대기 continuation + 공유 비동기 I/O 중개자**로 나눈다.
스레드 수, HTTP 동시 요청 수, 기다리는 작업의 메모리 예산은 서로 다른 한도다.

우선순위는 다음과 같다.

1. 원본 pattern의 정확한 합집합을 보존하면서 중복 graph 작업 자체를 줄인다.
2. local-first 경로에서 같은 작업 큐/continuation의 정확성을 확인한다. 파일 읽기와
   HTTP는 공급자만 다르며, graph/행 지우기/전체성의 권위를 나누지 않는다.
3. 온라인 경로는 요청 중복 결합·연결 재사용·독립 작업 진행을 함께 적용한다.
4. metadata/offset 의존 왕복을 줄이는 작은 블록 주소표를 A/B 후보로 삼는다.
5. native mmap은 별도 A/B, `.safetensors`는 당장 필요한 전제 조건이 아니다.

V*/policy/Krylov는 사용하지 않는다. 다섯 킥 프로필은 저장·캐시·완성 판정·파일
배치까지 독립적이어야 한다. 새로운 형식이나 작은 표본은 프로필 전체성 증명이 아니다.

## 2. 현재 정책: 비동기 reader와 비동기 탐색기는 다르다

| 구분 | 현재 소스 | 제안 방향 |
| --- | --- | --- |
| Web TB 실행 | `ClearraProductJobRunner`가 자원 lease를 얻은 뒤 `WasmJobRunner` 선택; TB 분기는 distributed prepare/run을 지나지 않음 | TB에도 독립적인 계산 작업을 배정하되 I/O 대기는 계산 슬롯에서 제외 |
| App 실행 | `Pc4OnlineHostExecution.pending: Option<RangeRequest>`가 있으면 advance가 즉시 Pending | 여러 작업의 대기 이유를 따로 보존하고 준비된 작업은 계속 진행 |
| 후보 생산자 | fixed-queue session의 `active_lookup: Option<ActiveLookup>` 하나 | 필드/블록의 공유 future와 그 응답을 기다리는 작업 목록 |
| Web 전송 | 기본 동시 4개, 설정 최대 16개, 대기열 512개; exact in-flight 중복 결합과 범위 캐시 존재 | 기존 제한/검증을 유지하면서 여러 계산 작업의 수요를 한곳에서 결합 |
| Web 실행 루프 | 단일 pending range의 prefetch와 read를 await한 뒤 다음 advance | 응답별 즉시 admission/wakeup; 한 batch 전체 완료가 다른 작업 진행 조건이 되지 않음 |
| CLI 전송 | 동기 host drive; 실제 범위 전송마다 curl 프로세스 생성·종료 대기 | host 수명의 공유 connection pool/비동기 전송 소유자 |

근거 소스:

- `apps/clearra-web/src/workers/ClearraProductJobRunner.ts:45`
- `apps/clearra-web/src/workers/WasmJobRunner.ts:72`
- `crates/clearra-app/src/pc4_online_host_execution.rs:177`
- `crates/clearra-app/src/online_pc4_fixed_queue_candidate_session.rs:426`
- `scripts/release/pc4/pc4-range-reader.mjs:14`
- `crates/clearra-cli/src/tablebase_host_execution.rs:15`
- `crates/clearra-cli/src/tablebase_download_transport.rs:112`

`readMany`의 Promise.all은 **transport 단계**에서 동시 요청이 가능하다는 증거다.
탐색에 CPU 워커가 여러 개 참여하거나 독립 graph branch가 계속 전진한다는 증거가
아니다. 프로필 초기 자격 검사에는 이미 병렬 batch가 있으므로 모든 TB 요청이
예외 없이 직렬이라고 표현하지 않는다. 이 진단은 특히 graph 조회/후보 생산 경계다.

## 3. 큐 정책과 정확성 계약

권장 흐름:

```text
준비된 계산 작업 -> CPU 워커 -> 완료 / 새 작업 / 필요한 필드·범위
                                                |
                                      공유 I/O 중개자
                               캐시·중복 결합·범위 묶음·전송
                                                |
                           검증된 응답 -> 해당 대기 작업만 다시 준비
```

- CPU 작업은 파일/HTTP를 기다리는 동안 계산 슬롯을 점유하지 않는다. 브라우저의
  전송 완료 이벤트를 처리하는 소유자에게 긴 동기 WASM 계산도 맡기지 않는다.
  짧고 유한한 계산 구간과 메시지 처리 기회를 보장한다. 단순히 `await`를 제거하면
  callback/cancellation을 굶기는 다른 형태의 지연이 생길 수 있다.
- 하나의 공유 I/O 소유자가 profile/revision/artifact/range별 요청을 결합한다.
  기존 exact 중복 외에 진행 중인 큰 범위에 포함되는 수요도 가능한 경우 결합한다.
  워커마다 동일한 graph/캐시를 복제하거나 독립 연결을 무제한 생성하지 않는다.
- 요청은 즉시 **등록**하되 항상 즉시 개별 전송하지는 않는다. 이미 알려진 인접
  수요를 비용 한도 안에서 합친다. 실행 가능한 CPU 작업이 사라졌거나 마지막
  의존 요청이면 batch가 찰 때까지 기다리지 않고 바로 전송한다.
- CPU의 기본 n-1/명시 전체 CPU/Cloud 정책과 I/O 동시성 M을 분리한다.
  네트워크 future 하나가 OS 스레드 하나를 요구하지 않는다. 전체 CPU 사용은
  응답 처리 소유자까지 긴 계산에 묶어도 된다는 의미가 아니다.
- 여러 전이가 같은 상태에 도착하면 동일한 불변 owner/입력/프로필/세대 안에서만
  합친다. 공급 상태뿐 아니라 부분 배치, graph 필드, 배치 깊이, 원래 행과의
  대응이 이후 의미를 결정한다. 아직 처리하지 않은 공급 상태 delta를 놓치거나
  이전 delta를 매번 전체 재계산하지 않도록 상태 버전과 처리 기록이 필요하다.
- 완료 조건은 ready queue가 빈 것만이 아니다. 계산 중/응답 대기/전송 중/
  미처리 delta가 모두 없고 생산자가 전체성을 봉인했을 때만 완료다. 동시 응답
  순서는 canonical 정렬, 최소 집합, 확률 분모 또는 tie의 의미를 바꾸지 않는다.
- 취소 epoch, lookup/request ID, 동일 generation, 정확한 범위와 길이 검증은
  그대로 유지한다. 늦게 도착한 응답은 새 작업에 admission하지 않는다.
- 원격 오류/부분 다운로드/한도 도달은 빈 해법이나 전체 탐색 완료가 아니다.
  네트워크 재시도나 offline fallback은 기존 명시 사용자 정책을 우회하지 않는다.

### 과잉 생산과 tail

중복 결합 후 생성되는 물리 요청률을 λ, 관측한 처리율을 μ라고 하면 λ > μ인 동안
대기열은 대략 `(λ-μ)t`만큼 증가한다. 계산이 빠를수록 무제한 제출이 더 위험하다.
작업 수뿐 아니라 continuation/응답 버퍼/캐시의 **바이트 예산**에 credit을 걸고,
수요를 늘리는 확장은 credit을 얻은 뒤 진행해야 한다. 회수된 응답 공간과 작업
완료가 credit을 반환한다. 원격 요청 총량 예산은 실패했다고 공짜로 되돌리지 않는다.

대략적인 병목 하한은 다음 항들의 최댓값이다. 이는 속도 보장이 아니라 모델이다.

`T >= max(C/P, R*L/M, B/W, 의존 경로 시간)`

여기서 C는 계산량, P는 실효 CPU 병렬도, R은 필요한 물리 요청 수, L은 대표 요청
지연, M은 실효 동시 요청 수, B/W는 전송량/대역폭이다. 긴 `offset -> graph -> 다음
필드` 의존 사슬은 M만 늘려도 사라지지 않는다. λ/μ가 일정하지 않은 실제 환경에서는
단일 평균 대신 지연 분포와 최대 적체량도 관측한다.

Tail 대책은 작업 훔치기 하나가 아니다.

- 작은 계산 조각은 idle CPU가 가져갈 수 있도록 한다. 이미 전송 중인 HTTP를
  다른 워커가 훔친다는 이유로 중복 발사하지 않는다.
- 오래 기다린 수요와 많은 작업을 깨울 수 있는 수요를 우선하되 starvation을 막는다.
- 독립 응답은 완료되는 대로 처리한다. 필요 없는 전체 batch/깊이 barrier는 두지 않는다.
  깊이별 합집합 봉인이 정확성에 필요한 구현이라면 barrier 제거 전에 delta/fixpoint
  계약을 먼저 갖춘다.
- 429/혼잡/timeout에서는 bounded backoff와 Retry-After를 고려하고 동시성을 줄인다.
  서버가 광고한 stream 상한을 요청 허용량으로 해석하거나 다른 origin으로 우회하지 않는다.
- 단 하나의 남은 원격 의존을 멀티스레딩으로 나눌 수는 없다. 해당 tail은 미리 확보한
  주소표, 공유 캐시, 더 짧은 의존 경로가 해결 대상이다.

## 4. HTTP/2·3 실확인

2026-09-14 공개 dataset API의 main revision은
`ea61380b31fa3dc9ffb4c8505c9a09c1b421ef31`였다. 기존 로컬 계측 세대와 같다.
이 값은 이번 계측의 출처이며 향후 업데이트를 금지하는 하드코딩 값이 아니다.

### 이 PC의 CLI용 curl

- `curl 8.21.0`, Windows/Schannel 빌드의 Features에는 HTTP2/HTTP3가 없다.
- 같은 immutable graph의 `Range: bytes=0-0` 실제 결과:
  `http_version=1.1`, `status=206`, `bytes=1`, `connects=2`, `redirects=1`,
  `total_s=1.133497`.
- 이는 현 PC의 CLI transport 결과다. 다른 OS의 curl, HF 서버 또는 브라우저가
  HTTP/1.1만 지원한다는 뜻이 아니다. 소스는 curl 프로세스를 물리 요청마다
  생성하므로 HTTP/2 지원 curl로 바꾸기만 해도 요청 간 connection pool이 생기지는 않는다.

### 서버와 같은 연결의 병렬 Range

Node의 HTTP/2 client로 공개 URL을 HTTPS/허용된 HF host 안에서만 따라갔다.
redirect body는 헤더 수신 시 중단하고, 최종 body는 작은 상한으로 제한했다.
첫 진단은 redirect 본문 상한에 걸려 중단됐으며 결과에 포함하지 않았다.
보정된 진단은 다음을 확인했다. 서명된 redirect URL/쿼리 값은 기록하지 않았다.

| 구간 | 협상 프로토콜 | 응답 | 광고한 최대 동시 stream | Alt-Svc |
| --- | --- | --- | --- | --- |
| huggingface.co | h2 | 302 | 128 | h3 광고 |
| us.aws.cdn.hf.co | h2 | 206, 정확한 1바이트 | 100 | h3 광고 |

최종 CDN의 **같은 연결**에 `bytes=0-0`, `bytes=1-1`을 함께 제출했다.
각각 `206`, 정확한 Content-Range와 1바이트, 628ms/624ms였고 두 요청의
벽시계 합계는 628ms였다. 병렬 Range가 실제 지원된다는 확인이지 검색 A/B,
네트워크 지연의 평균 또는 100개 요청을 동시에 보내도 된다는 근거는 아니다.

HTTP/3는 지원 광고만 확인했고 QUIC로 실제 연결하지 않았다. 브라우저 연결 목록이
비어 있어 실행 중인 GUI의 협상 프로토콜도 확인하지 못했다. 탐색 재실행/탭 교체는
하지 않았다. 브라우저 fetch는 연결 협상을 브라우저에 맡긴다. 제품에는 가능할 때
`nextHopProtocol`을 관측하고, 보안상 가려진 빈 값은 unknown으로 남기는 편이 맞다.
[Resource Timing](https://www.w3.org/TR/resource-timing/)

HTTP/2는 stream multiplexing을 지원하지만 TCP 손실의 연결 전체 지연은 남는다.
HTTP/3는 QUIC의 stream별 전송을 이용하지만 원격 처리 시간, 공유 대역폭, 요청
의존 사슬을 없애지는 않는다. 둘 다 Range의 의미와는 별개 계층이다.
[HTTP/3 RFC 9114](https://www.rfc-editor.org/rfc/rfc9114.html)

Native의 우선 후보는 지속적인 비동기 client/pool + HTTP/2 협상이며, HTTP/3는
실제 라이브러리/환경 지원이 검증된 경우의 추가 경로다. libcurl을 유지한다면
multi handle에서 연결을 공유하는 방식이 가능하지만 지금의 매번 새 CLI process와
같지 않다. HTTP/1.1 환경의 bounded fallback도 유지한다.
[libcurl multiplexing](https://curl.se/libcurl/c/CURLMOPT_PIPELINING.html)

`Range: bytes=a-b,c-d`의 multipart 요청과 여러 단일 Range의 HTTP/2 stream은
다르다. 이번에 검증한 것은 후자다. 현 reader는 단일 정확 Content-Range 계약이므로
multipart 서버 지원/브라우저 CORS/파서를 검증하지 않고 혼용하지 않는다.

## 5. 메타데이터 캐시와 블록 주소표

현재 GUI는 준비한 generation을 모듈 안에서 5분 재사용하고 진행 중 discovery를
결합한다. CLI 온라인 경로는 실행마다 revision/tree metadata를 다시 조회한다.
이는 개선 가능하지만 수만 회의 graph/index 요청과 비교하면 작은 비용이다.
**세대 메타데이터**와 **필드 주소 인덱스**를 구분해서 최적화해야 한다.

- `(repository, immutable revision, profile, artifact identity, layout)`로 파일/인덱스
  캐시를 나눈다. 각 실행은 세대를 고정하고 후속 실행에서 업데이트를 확인한다.
  TTL 재확인과 immutable 데이터 수명을 분리하며 무효한 세대를 정상으로 계속 쓰지 않는다.
- 서명된 redirect URL을 파일의 영구 주소/identity로 저장하지 않는다. connection
  pool 재사용과 expiring URL 캐시는 다른 문제다.
- 기존 Jstris 세 파일: FHID 121,485,664B, GOFF 60,742,844B, graph 510,917,451B.
  record 수는 15,185,706이다. 이 graph의 수를 layer-0 quotient 817,740과 혼동하지 않는다.
- GOFF의 연속 u32 두 개로 graph record의 시작/끝을 알아낸다. offset이 캐시에 없으면
  graph 요청 전 별도 응답이 필요하다. 메타데이터 준비만 빠르게 해도 이 의존은 남는다.

### 기존 graph를 바꾸지 않는 후보: 64-record sparse directory

기존 로컬 GOFF 파일만 한 번 읽어 content identity와 모든 offset의 순서/최종
sentinel을 검증했다. graph 전체를 다시 내려받거나 PC 탐색을 재실행하지 않았다.
64개 ID씩 묶은 블록의 경계 offset만 보관하면 다음과 같다.

| 항목 | 실제 offset 파일에서 계산한 값 |
| --- | --- |
| 블록 수 | 237,277 |
| 경계 u32 배열 크기, header 제외 | 949,112B |
| 블록 크기 min / median | 654B / 1,812B |
| 블록 크기 p95 / p99 / max | 4,416B / 6,888B / 21,141B |
| 블록 평균 크기 | 2,153.25B |
| 기존 개별 record min / max | 12B / 498B |

계산은 `4 * (ceil(15,185,706/64) + 1)`이다. 블록 내부는 이미 self-delimiting
piece별 degree/target 구조이므로 최대 64개 record를 읽어 원하는 ordinal을 찾을 수
있다. 알려진 target ID에 대해서는 작은 주소표를 미리 캐시한 뒤 graph block 한 번으로
진행할 수 있다. 임의 초기 필드의 hash -> ID 검색은 여전히 별도이며 이 표가 대체하지 않는다.

현재 자격을 가진 3-byte target/누적 degree <=255 구조에서는 record 상한이
`5 + 7 + 3*255 = 777B`, 64개 상한이 49,728B다. 이 상한과 실제 최대값은 다른
근거다. SRS-X는 target 폭이 4바이트인 별도 형식이므로 같은 64개를 무조건 적용하지
않는다. 프로필마다 폭/degree/byte cap을 검증해 블록 크기를 결정한다.

이것은 **성능 성공 판정이 아니다**. 무작위 희소 조회에서는 쓰지 않을 이웃 bytes가
늘고 총 64MiB 예산을 더 빨리 소모할 수 있다. 큰 known frontier의 블록 재사용률,
exact read 대비 추가 bytes, 줄어드는 offset 왕복을 비교해 작은/혼합/전체 저장 경로를
선택해야 한다. 이전 16KiB 일괄 선읽기의 실패를 재현하지 않는다. 한도 초과를 허용하거나
작은 탐색에서 60MB 주소표 전체 다운로드를 강제하지 않는다.

이 보조 표는 내려받은 기존 GOFF에서 Clearra가 로컬로 만들 수 있다. 온라인 사용자에게
작은 표만 제공할 필요가 입증되면 upstream에 보조 파일 추가를 요청한다. graph의 ID,
간선, 순서, 킥 의미를 바꾸지 않아 기존 파일 및 소비자와 공존할 수 있다.

## 6. safetensors와 mmap

### `.safetensors`: 포맷 전환만으로 요청 수가 줄지는 않는다

형식은 길이 prefix, JSON tensor header, dtype/shape/data_offsets와 연속 data buffer를
정의한다. metadata만 Range로 읽는 방법도 제공하지만 header 조회 뒤 payload가 필요한
의존은 남는다. ragged graph는 offsets와 packed target 배열/byte tensor로 표현해야 한다.
각 필드를 tensor 하나로 만들면 1,518만 개 JSON entry가 생기므로 후보에서 제외한다.
[형식 명세](https://github.com/safetensors/safetensors/blob/main/README.md),
[metadata 부분 조회](https://github.com/safetensors/safetensors/blob/main/docs/source/metadata_parsing.mdx)

현재 graph/offset/index는 이미 binary random access가 가능하다. u24 target을 u32로
단순 확장하면 target payload가 33.3% 증가하고, FHID의 5-byte hash+3-byte ID를
u64+u32로 확장하면 record가 8B -> 12B로 50% 증가한다. packed U8 tensor로 보존하면
이 증가는 피하지만 자체 해석기가 계속 필요하다. safetensors가 압축·멀티플렉싱·
graph 인덱싱·전체 해법 증명을 자동으로 제공하지는 않는다. 이 비교에서는 우선하지 않는다.

### mmap: native 로컬 조회의 별도 후보

CLI는 현재 `Read + Seek`, 4KiB index page와 최대 512개 page/file 캐시를 쓰고 graph는
정확 범위로 읽는다. mmap은 기존 `.bin` 그대로 적용 가능하다. worker마다 seek cursor를
공유해 lock 경합을 만드는 대신 positional read 또는 공유 read-only map을 비교한다.

단, mmap은 원격 HTTP 왕복의 해결책이 아니며 첫 page fault/I/O와 decode/복사 비용은
남는다. 메모리 압박이 있는 대형 데이터에서 무조건 빠르다고 하지 않는다. 수정 가능한
파일의 map은 안전하지 않으므로 실행 중 immutable generation lease, 삭제/교체 금지,
범위/정렬/endianness 검사, 명시적 수명 종료가 필요하다.
[memmap2 안전성 계약](https://docs.rs/memmap2/latest/memmap2/struct.MmapOptions.html)

GUI의 OPFS read-only sync access handle 재사용은 이미 별도 변경에 들어갔지만
브라우저의 OS mmap 또는 WASM zero-copy와 같지 않다. browser/Native는 파일 transport
어댑터를 분리하면서 검증된 byte slice 이후의 graph/Core 의미를 공유한다. native
mmap 실행 계측은 아직 없고 현재 로컬 실행 보안 정책을 우회하지 않는다.

## 7. A/B 및 upstream 요청의 판단 기준

기존 10만 논리 조회의 45,551 -> 44,443 물리 요청(2.43% 감소) 결과는 재사용한다.
이는 cache-neighbor 묶음의 로컬 응답 재생 결과이지 실제 WAN/전체 탐색 성능이 아니다.
이번 단일 연결 진단 또한 그 A/B와 합치지 않는다.

새 실험은 다음 요인을 하나씩 바꾼다.

1. 같은 local dataset에서 기존 생산자 vs 정확 compact union/독립 continuation.
2. 같은 작업/전송 trace에서 직렬 demand vs bounded M=1/4/8/16 비동기 broker.
   먼저 로컬 지연 모델에서 확인하고 실제 서비스에는 보수적인 동시성부터 적용한다.
3. 기존 GOFF paging/exact graph vs sparse block directory와 bounded 혼합 조회.
4. native positional read vs read-only mmap. 이를 HTTP/알고리즘 개선과 섞지 않는다.

source/WASM identity, CPU 작업 수와 HTTP stream 수, 고유 필드/요청 수, 중복 결합,
전송 bytes, pool 연결 수, 협상 프로토콜, ready/waiting 최대 bytes, p50/p95/마지막
5% 시간을 따로 기록한다. 작은 입력의 회귀와 큰 입력의 cold/warm 차이를 모두 본다.
빈 필드 /4L/Jstris 180/P7P4 전체 456,459개는 기존 해법 집합 대조 기준으로 유지하며
count만 강제로 맞추거나 일부 완료를 전체 완료로 표현하지 않는다.

muse918에게 요청할 수 있는 최소 변경은 다음 두 가지다. 아직 전송하지 않았다.

- 프로필별 completion/target 폭/field 수/terminal ID/kick·schema·file identity를
  기록한 기계 판독 가능 generation manifest. 새 upstream revision을 허용하되
  한 실행에서 서로 다른 revision/profile의 파일을 섞지 않는다.
- 기존 graph 및 ID 순서를 유지하는 작은 block-boundary sidecar. Jstris의 64-record
  후보는 header 제외 약 0.95MB이고, 다른 프로필은 별도로 크기를 정한다. 로컬
  A/B로 이득이 없으면 요청하지 않는다. 필요하면 bounded block digest도 별도 검토한다.

현 단계에는 전체 graph의 safetensors 변환, 파일 재정렬 또는 새 서버 batch API를
요청할 필요가 없다. 비동기 탐색기/공유 client/캐시는 Clearra 내부에서 진행 가능하다.

## 8. 2026-09-19 연결 구현의 범위

이후 compact 합집합/협력적 finalizer는 `b23cb5e`의 비게시 CI에서 통과했다.
후속 변경은 격리된 구현 브랜치의 compact pattern PC host에 독립 조회 owner를
연결한다. 다음 값들은 CPU 워커 정책 또는 upstream 허용량을 뜻하지 않는다.

| 경계 | 구현 제한/정책 |
| --- | --- |
| App lookup | 활성 최대 8개, 필드별 하나; 응답 대기는 다른 lookup과 ready CPU를 막지 않음 |
| CPU backpressure | 대기 16필드 또는 64 continuation에서 확장 일시 중단; 응답 뒤 재개, work quantum 최대 64 |
| Web continuation | 최대 16개, 각 응답 최대 64KiB; admission 전 완료 버퍼도 같은 slot을 점유 |
| HTTP | 기존 4개 물리 요청 상한/byte reservation/cache/세대 identity 유지 |
| 알려진 인접 수요 | 같은 프로필/파일 안에서만 4KiB gap, 최대 64KiB 단일 범위로 묶음; 새 수요를 기다려 batch를 채우지 않음 |
| 응답 처리 | 먼저 완료된 span의 요청부터 admission; 전체 batch의 Promise.all 대기 없음 |
| local | 이미 검증된 파일의 page/exact read 유지; HTTP 상태를 만들지 않으며 close 시 미완료 read를 먼저 정리 |

브라우저 host/OPFS 관련 22개 Node 계약과 기존 reader 29개 계약을 로컬에서 통과했다.
느린 첫 응답 전에 다른 두 응답이 admission되는 것, I/O 중 CPU 진행, 8개 논리 수요가
4개 물리 요청 상한을 지키는 것, 취소/HTTP 200/초과 batch의 실패 처리를 확인했다.
인접 수요 3개는 HTTP 1회로 읽고 원래 세 요청에 따로 admission했다. 이들은 통제된
fixture의 계약 검증이지 WAN 시간 또는 456,459개 전체 탐색 가속 증거가 아니다.

아직 구현되지 않은 경계도 구분한다. CPU work-stealing, 깊이 장벽 없는 delta 스케줄링,
whole-owner 바이트 credit, CLI 지속 연결/비동기 client, sparse sidecar, mmap은 남아 있다.
count watermark와 개별 버퍼 한도가 전체 RSS/사용자 memory budget을 대체하지 않는다.
새 Rust host/다중 세션 계약은 exact-source 비게시 CI에서 확인한다. 4194/배포는 변경하지
않으며 HTTP 프로토콜 진단도 다시 실행하지 않았다. 4절의 네트워크 관측일은 그대로다.

### 8.1 2026-09-19 추가 소스 점검: 헤더 캐시의 계층

현재 `LookupMachine::start_with_selector`의 qualified field-ID 경로는 lookup마다
`GOFFIDX1` 헤더 16바이트를 먼저 요청한 다음 offset pair와 graph record로 진행한다.
`consume_offset_index_header`가 각 세션에서 동일 version/count를 재검증한다.
따라서 기존 4KiB host 캐시는 물리 HTTP/파일 읽기를 피할 수 있어도, 새 lookup의
헤더 수요·JSON 전달·admission 비용까지 없애지는 않는다. 알려진 ID에 대한 보통
3회 논리 요청 중 1회가 이 헤더다. 이는 실제 prefix의 graph record 약 3배 논리
조회와도 일치하지만, 아직 헤더 공유 전후 실행시간 A/B를 수행한 것은 아니다.

다음 후보는 같은 immutable snapshot/profile/artifact/layout에서 한 번 검증한
**index-header witness**를 lookup owner 사이에 공유하는 것이다. 단순 boolean,
파일명만의 global cache, Web에서 가짜 응답 생성, 형식 검증 삭제로 대체하지 않는다.
source/snapshot 취소와 프로필/세대 변경 시 이전 witness를 사용할 수 없어야 한다.
이 후보는 초기 generation discovery 캐시와 별개이며, 캐시 적중 상태의 논리 왕복을
줄이는 것이다. `offset -> graph`의 실제 원격 의존을 줄이는 block directory와도
구분해 A/B해야 한다. 이번 frontier 수정에는 아직 이 witness를 넣지 않았다.

이 PC의 CLI curl 기능 목록도 2026-09-19 다시 확인했으며 HTTP2/HTTP3가 없다.
CLI 소스의 요청별 process 생성/동기 대기도 그대로다. HF h2 병렬 Range 및 h3 광고는
4절의 2026-09-14 관측을 재사용하며 새 네트워크/GUI 계측으로 표현하지 않는다.
HTTP/2·3의 stream multiplexing, libcurl multi의 공유 연결, safetensors의 offset
header 및 memmap2의 file-mutation 안전성 계약은 해당 공식 문서와 다시 대조했다.
이 검토로 `.safetensors` 변환이나 실제 QUIC/native mmap 구현이 완료되는 것은 아니다.

## 9. 2026-09-19 재비교: 검증 소스, 작성 중 후보, 실제 전송을 분리

이번 비교의 커밋 기준은 `def79304f59685e9ba3d5ae21d2f01dbc7f4f2b0`이다.
그 HEAD는 직전 소스 후보 `1e4f9f4`의 검증 결과를 기록한 문서 변경이다. 아래의
미커밋 cache/resident 변경은 아직 Rust 컴파일·실행 검증을 마치지 않은 후보이며,
HEAD의 통과한 CI 또는 4194에 반영된 동작으로 취급하지 않는다. 이번 재비교에서는
기존 Rust 변경을 보존하고, 탐색 재실행·4194 교체·CI·배포를 수행하지 않았다.

### 9.1 현재 경계와 제안의 차이

| 구분 | 현재 커밋에서 확인한 정책 | 남은 변경/검증 |
| --- | --- | --- |
| compact pattern App | 서로 다른 필드의 lookup 최대 8개; 같은 필드는 하나의 활성 lookup에 결합 | CPU 작업 훔치기와 별개이며 CPU 병렬 실행 증거가 아님 |
| Web host | `batch`와 `can_advance`를 소비; 첫 완료 응답부터 admission, I/O 중 ready 계산 진행 | TB 자체는 `WasmJobRunner` 한 개이며 일반 PC의 distributed 경로를 사용하지 않음 |
| Web 전송 | 물리 HTTP 기본 4개(설정 상한 16); exact in-flight 결합, byte 예약, index page 캐시 | 요청 M과 CPU P를 각각 계측/조정; 큰 in-flight span에 포함된 다른 키의 수요 결합은 별도 후보 |
| 응답 버퍼 | 최대 16개 논리 slot, 각각 최대 64KiB; 완료됐어도 admission 전까지 slot 점유 | 이 제한은 전체 작업·캐시·결과의 메모리 상한이 아님 |
| 기존 ready 제어 | waiting 16필드 또는 64 continuation에서 union 진행도 제한; graph admission 때 wakeup 허용 | 새 수요를 만드는 cold 확장만 제한하고 기존 resident/응답 처리 진행은 보장할 필요 |
| 그 외 경로 | legacy/fixed-queue 경로에는 단일 pending await가 남음; CLI는 동기 drive와 요청별 curl 프로세스 | 모든 PC/Setup/CLI/Discord가 이미 비동기라고 표현하지 않음 |
| 큰 입력 | 누적 lookup 100,000개와 append-only graph cache 한도가 별도로 존재 | 동시성 상한으로 오해하지 말고 bounded replacement와 전체 집합 정확성을 함께 검증 |

작성 중인 후보는 resident work 최대 64개와 source/current-target pin을 두고,
비보호 항목만 CLOCK 방식으로 교체한다. record 수가 교체 전후 같아도 응답 도착을
놓치지 않도록 admission revision으로 깨운다. 누적 lookup을 cache 크기로 제한하는
대신 이미 부과된 graph work 예산과 연결하며 HTTP 총 요청/전송 예산은 유지한다.
이 후보의 eviction·응답 순서·취소·큰 입력 전체 집합 검증은 아직 남아 있다.

CPU가 네트워크보다 빨리 수요를 생성할 때의 해결책은 무제한 제출도, 전체 CPU 정지도
아니다. 신규 확장에는 byte credit을 요구하고, 받은 응답 반영·기존 작업 완료·pin 해제는
독립적으로 진행해야 한다. 제어 메시지/취소를 처리하는 host도 긴 동기 계산으로 막지
않는다. 다수 continuation이 같은 필드를 기다릴 때 요청은 한 번만 만들고 모두 깨운다.

Tail은 세 가지로 구분한다: (1) 남은 큰 CPU 작업은 유한 조각 분할/작업 훔치기,
(2) 느린 단일 HTTP는 먼저 온 다른 응답 처리와 공정한 수요 우선순위,
(3) `offset -> graph -> 다음 필드` 사슬은 주소표/캐시로 의존 왕복을 단축한다.
정확한 layer 합집합을 봉인하기 위한 장벽은 delta/fixpoint 증명 없이 제거하지 않는다.

대략적인 비교 모델은 `T >= max(C/P, R*L/M, B/W, T_dependency)`다. 이는 일정한
대표 지연 L과 실효 동시성 M을 가정한 병목 모델이지 실행시간 예측이나 속도 보장이
아니다. CPU 추가보다 실제 물리 요청 R을 줄이는 효과가 클 수 있다. 요청 발생률이
처리율보다 높으면 queue가 계속 커지므로 ready/waiting/result까지 포함한 bytes를
계측해야 한다. 여러 인접 요청을 합칠지는 줄이는 왕복 비용과 추가 bytes/파싱 비용을
비교하되, 마지막 의존이나 빈 CPU 큐가 batch 충전을 기다리지 않게 한다.

### 9.2 이번 최소 네트워크 진단

2026-09-19 10:29:17 UTC에 공개 metadata로 조회한 revision은 여전히
`ea61380b31fa3dc9ffb4c8505c9a09c1b421ef31`이었다. 이 식별자는 관측 출처이며
업데이트 금지 설정이 아니다. 기존 graph 크기도 510,917,451B와 일치했다.

Node HTTP/2로 HTTPS HF origin과 허용된 HF CDN만 따라가며 metadata 최대 4MiB,
Range 응답 각각 1B, redirect 최대 5회, 전체 진단 45초 상한을 적용했다. redirect
본문은 중단하고 서명 URL/쿼리는 기록하지 않았다. 마지막에 모든 연결을 닫았다.

| 관측 | 결과 |
| --- | --- |
| `huggingface.co` | h2 / 302 / 광고 stream 상한 128 |
| `us.aws.cdn.hf.co` | h2 / 206 / 광고 stream 상한 100 / 첫 1B 확인 1,317.554ms |
| 같은 CDN 연결의 `bytes=0-0` | 206 / 정확한 Content-Range·1B / 704.755ms |
| 동시에 제출한 `bytes=1-1` | 206 / 정확한 Content-Range·1B / 700.880ms |
| 위 두 요청의 벽시계 | 706.032ms |

이는 **단일 연결의 병렬 Range 지원** 확인이다. 새 전체 탐색/A-B, 평균 WAN 지연,
100개 동시 요청 허용량, 실제 GUI 프로토콜의 증거가 아니다. 이번 관측에서는 h3
Alt-Svc 광고를 보지 못했다. 4절의 9월 14일 광고 관측을 지우거나 현재 관측과 혼합하지
않으며, 어느 쪽도 실제 QUIC 성공 또는 미지원 확정을 뜻하지 않는다.

이 PC의 `curl 8.21.0` Features에는 여전히 HTTP2/HTTP3가 없다. 요청마다 프로세스가
끝나는 CLI 소스도 그대로다. HTTP/2 지원 curl로 교체하는 것만으로 프로세스 간 연결
재사용이 생기지는 않는다. 지속 client/pool 또는 libcurl multi가 필요하다.
브라우저 도구는 연결됐지만 탭 목록이 비어 있어 실제 GUI의 protocol은 unknown으로
남겼다. 탭 생성/새로고침/탐색/다른 브라우저 전환은 하지 않았다.

HTTP/1.1도 여러 연결로 비동기 동시 요청을 할 수 있다. HTTP/2·3은 CPU 스레드 기능이
아니라 공유 연결의 stream 기능이다. HTTP/2의 TCP 손실에 따른 지연과 HTTP/3의
stream 분리는 [RFC 9113](https://www.rfc-editor.org/rfc/rfc9113.html)와
[RFC 9114](https://www.rfc-editor.org/rfc/rfc9114.html)에 구분된다. Native는 우선
[공유 multi/pool](https://curl.se/libcurl/c/CURLMOPT_PIPELINING.html)과 h2 협상,
h3는 실제 환경 검증 이후의 추가 경로로 둔다. `Range: a-b,c-d` multipart 지원을
이번 두 단일 Range의 stream 검증으로 대신하지 않는다.

### 9.3 포맷/메타데이터/mmap 우선순위 재확인

1. **Clearra 내부 변경 우선:** bounded continuation과 응답 처리 진행 보장,
   graph cache 교체 정확성, CLI의 지속 연결/비동기 drive. 포맷 변경이 필요 없다.
2. **논리 header 왕복 제거:** immutable profile/generation/artifact/layout에 묶인
   검증된 GOFF-header witness를 공유한다. 5분 discovery 캐시나 4KiB byte 캐시는
   이미 있지만 lookup마다 헤더를 요청하는 App/ABI 왕복은 여전히 남는다.
3. **물리 offset 왕복 제거:** 기존 64-record sidecar 후보(949,112B, header 제외)를
   large-frontier A/B 대상으로 유지한다. 순서/ID/간선을 바꾸지 않고 known ID의
   block 위치를 정한다. 임의 초기 필드의 hash->ID 검색은 별개다. 기존 로컬 GOFF로
   먼저 만들 수 있고, 이득이 확인된 뒤 upstream에 작은 sidecar 추가를 요청한다.
4. **mmap은 native local 전용 별도 A/B:** 기존 `.bin` 그대로 positional read와
   비교한다. GUI OPFS handle/Blob slice, HTTP, WASM zero-copy와 혼동하지 않는다.
   immutable generation lease와 파일 변경/삭제 방지, 읽기 범위/수명 검증이 전제다.
   [memmap2](https://docs.rs/memmap2/latest/memmap2/struct.MmapOptions.html)의
   file-backed map 안전성 요구는 단순 `read-only` 옵션만으로 충족되지 않는다.
5. **safetensors는 후순위:** offsets/packed bytes를 tensor 몇 개로 넣을 수 있으나
   graph 주소 조회와 의존 사슬은 그대로다. 필드마다 JSON tensor entry를 만드는
   1,518만-entry header는 피한다. u24를 u32로 넓히면 target payload +33.3%,
   5B hash+3B ID를 u64+u32로 바꾸면 해당 index record +50%다. U8 packing으로
   보존하면 크기 증가는 피하지만 기존 decoder가 필요하다. 이 구조적 판단은
   [공식 포맷](https://github.com/safetensors/safetensors/blob/main/README.md)과
   [부분 metadata 조회](https://huggingface.co/docs/safetensors/metadata_parsing)를
   다시 대조했으며 변환 실행/속도 개선 증거는 아니다.

다섯 킥 프로필마다 자격·파일 폭·캐시·명시 다운로드를 분리한다. muse918에게 보낼
후보는 여전히 profile/schema/completion manifest와 기존 graph 호환 block directory다.
필요시 block digest를 더할 수 있지만 새로운 세대/블록의 검증 계약이 먼저다.
V*/최선 수/Krylov나 부분 집합으로 대체하지 않으며, 요청 메시지는 아직 보내지 않았다.

### 9.4 bounded cache 검증과 새 대형 로컬 계측

후보 구현은 `9f77a6a`에서 다음 경계를 제품 코드에 반영했고, 테스트 전용 private
관측 경계만 바로잡은 정확한 소스는 `4840a3278c46a1be8f392b63b6eed94056f48c6e`다.

- 신규 CPU 확장만 최대 64 resident work credit으로 제한한다.
- work의 source와 현재 target record를 pin하고, 응답 처리·기존 resident 완료는
  credit이 가득 찼어도 계속 진행한다.
- cache는 pin되지 않은 record만 CLOCK 방식으로 교체하며 record 수가 같아도
  admission revision으로 대기 work를 깨운다.
- 누적 lookup을 cache 크기와 같게 제한하던 100,000회 상한은 제거한다. 새 lookup은
  이미 부과된 유한 graph work에서만 파생되고, HTTP 요청·byte 및 App 작업 예산은
  계속 독립적으로 적용된다.

[비게시 CI 35438327542](https://github.com/daejunnom/Clearra/actions/runs/35438327542)의
source, surface, native CLI, PC4 계약, preview-WASM 다섯 job이 모두 성공했다. cache
교체 전 검증/예산/예약의 원자성, pin 보존, admission 순서, 취소 시 pin 해제, 2-record
cache에서도 작은 4L exact candidate/count/digest 동등성, bounded diamond 탐색을
검사했다. 이는 릴리스 acceptance나 대형 전체 집합 증명이 아니다.

해당 실행의 메모리 직접 importer와 기존 로컬 Jstris 180 자산으로 빈 필드/P7P4/
4L/unique 전체 탐색을 한 번 실행했다. 240초보다 먼저 1,000,000 논리 조회 한도에
도달해 안전하게 취소했으며, 알려진 456,459개 전체 집합 또는 완료를 주장하지 않는다.

| 지표 | 새 bounded-cache probe |
| --- | ---: |
| 벽시계 | 92.720초 |
| 논리 조회 / 물리 파일 읽기 | 1,000,000 / 390,678 |
| 파일 bytes / cache hit | 273,225,460B / 609,667 |
| FHID / GOFF / graph 논리 조회 | 2 / 666,666 / 333,332 |
| WASM 계산 / local I/O / bridge | 28.701초 / 25.993초 / 35.717초 |
| 최대 한 번의 WASM advance | 20.701ms |
| 마지막 WASM memory | 196,935,680B |

이 계측은 기존 100,000 lookup 종료 지점을 실제로 넘었고, append-only graph cache의
무제한 증가 대신 bounded 교체가 작동했음을 보인다. 반면 거의 정확한
`GOFF pair 두 논리 호출 -> graph record 한 호출`이 계속되어 bridge가 가장 큰 누적
구간이었고, 전체 완료 전에 333,332 graph demand가 발생했다. 로컬 직렬 benchmark
host의 수치이므로 Web의 first-completion async pump나 인터넷 HTTP 시간을 대신하지
않는다. 그래도 CPU worker 수만 늘리는 것보다 typed header/address witness, batch
admission, block directory로 논리·물리 호출을 줄이는 순서가 먼저라는 근거다.

다음 A/B는 이전 100k frontier 결과를 반복하지 않는다. 같은 기존 graph/GOFF로
만드는 프로필별 sparse block directory가 known ID의 graph block을 GOFF 응답과
동시에 준비할 수 있는지와, 여러 response를 한 ABI admission으로 넘겼을 때 bridge
호출 수가 실제로 줄어드는지를 각각 분리한다. sidecar가 large-frontier에서 요청 수와
tail을 유의미하게 줄일 때만 muse918에게 동일 generation/profile/layout identity 및
완성 manifest에 결박된 upstream 보조 파일을 요청한다.

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
| 그 외 경로 | 온라인 제품 CLI는 persistent multi/pool과 `pending_ranges()`를 소비한다. 로컬 파일 driver와 합성 test adapter만 scalar `pending_range()`를 유지한다. 순차 fixed-queue의 한 의존 요청은 병렬 host 누락이 아니라 graph dependency다. | 모든 lookup이 서로 독립이라고 과장하지 않고 큰 전체 집합의 mixed index/graph tail을 실제 A/B할 것 |
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
3. **물리 offset 왕복 제거:** 실제 demand trace 모델에서 bytes/요청 균형이 가장 좋았던
   Jstris 16-record sidecar 후보(3,796,448B)를 large-frontier A/B의 첫 대상으로 둔다.
   순서/ID/간선을 바꾸지 않고 known ID의 block 위치를 정한다. 임의 초기 필드의
   hash->ID 검색은 별개다. 다른 프로필은 각각의 qualified GOFF/graph에서 K를 다시
   선택하고, 이득이 확인된 뒤 upstream에 작은 sidecar 추가를 요청한다.
4. **mmap은 native local 전용 별도 A/B:** 기존 `.bin` 그대로 positional read와
   비교한다. GUI OPFS handle/Blob slice, HTTP, WASM zero-copy와 혼동하지 않는다.
   immutable generation lease와 파일 변경/삭제 방지, 읽기 범위/수명 검증이 전제다.
   [memmap2](https://docs.rs/memmap2/latest/memmap2/struct.MmapOptions.html)의
   file-backed map 안전성 요구는 단순 `read-only` 옵션만으로 충족되지 않는다.
   witness 이후 probe에서 local I/O 18.265초를 전부 0으로 만드는 비현실적 상한조차
   72.993초에서 약 54.727초(1.33배)다. page fault와 결과 복사는 남으므로 실제 mmap
   기대치는 그보다 작으며, 원격 왕복/bridge보다 먼저 구현할 근거는 없다.
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

### 9.5 header witness 구현과 sparse block-directory 모델

`8d17b25`는 최초 조회에서 검증한 GOFF header를 단순 boolean이 아니라
snapshot/profile/artifact descriptor/field count에 결박된 typed witness로 만들었다.
후속 field-ID lookup은 같은 witness가 정확히 일치할 때만 GOFF header 논리 요청을
생략한다. 첫 비게시 CI는 inline Hydra 레이아웃만 허용한 조건 때문에 synthetic opaque
레코드 계약 3개를 거절했다. `e8b6bfe`는 opaque 레이아웃에서 기존 FHID header/record
검증은 유지하고 GOFF header만 건너뛰도록 바로잡았다. 즉 source hash가 graph record에
inline인 Jstris 경로는 `offset pair -> graph`, opaque 경로는
`FHID header/record -> offset pair -> graph`가 되며, 어느 쪽도 세대·프로필·필드 ID
검증을 생략하지 않는다. [exact-source 비게시 CI 35439887487](https://github.com/daejunnom/Clearra/actions/runs/35439887487)의
source, surface, native CLI, PC4 계약, preview-WASM 다섯 job이 모두 성공했다.

그 preview-WASM과 같은 로컬 Jstris generation으로 기존 probe와 같은 입력을
**333,332 graph demand 지점**까지 다시 실행했다. 이전 probe의 demand digest는 보존되지
않았으므로 exact 순서 동등성을 주장하지 않고, 비교 기준은 같은 입력과 같은 graph
논리 조회 수로 제한한다. 두 실행 모두 완료 전에 안전하게 취소됐으므로 456,459개 전체
집합·digest·완료 증거도 아니다.

| 지표 | 반복 header | typed witness | 변화 |
| --- | ---: | ---: | ---: |
| 벽시계 | 92.720초 | 72.993초 | -21.3% |
| 논리 조회 | 1,000,000 | 666,667 | -333,333 |
| 물리 파일 읽기 | 390,678 | 390,674 | 사실상 동일 |
| 파일 bytes | 273,225,460B | 273,209,391B | 사실상 동일 |
| FHID / GOFF / graph 논리 조회 | 2 / 666,666 / 333,332 | 2 / 333,333 / 333,332 | header 반복 제거 |
| WASM 계산 | 28.701초 | 26.964초 | -6.1% |
| local I/O | 25.993초 | 18.265초 | -29.7% |
| host/WASM bridge | 35.717초 | 25.932초 | -27.4% |

물리 읽기/bytes가 변하지 않았는데 bridge와 benchmark-host I/O가 줄었으므로, 이 후보의
주효과는 원격 데이터를 덜 받는 것이 아니라 캐시된 GOFF header를 매 lookup마다
App/WASM 논리 응답으로 다시 전달하던 비용을 없앤 것이다. 실제 HTTP 왕복 수를 줄이는
다음 단계는 아래 block directory 또는 서버 batch endpoint이며 별도 A/B가 필요하다.

정적 HF/CDN에 `Range: bytes=0-0,2-2`를 보낸 1KiB/20초 bounded probe는 body 없이
HTTP 416을 반환했다. 따라서 multipart/byteranges 한 요청으로 흩어진 graph record를
묶는 설계는 현재 upstream 경로의 근거가 없다. 같은 h2 연결의 여러 **단일** Range
stream을 쓰는 기존 결론은 유지한다.

기존 30,000 real logical-demand trace(9,999 exact graph demands)와 로컬 Jstris generation을
읽기 전용으로 사용해 sparse block-directory를 새로 모델링했다. 모델은 16B header,
block 시작마다 u32 graph offset, 4KiB sidecar page, graph block span, 공유 8MiB/2,048-entry
LRU 및 64KiB 단일 Range 상한을 적용한다. 검색/WAN 시간이나 sidecar 구현 완료가 아니다.

| block records | sidecar bytes | modeled requests | modeled bytes | 최대 block |
| ---: | ---: | ---: | ---: | ---: |
| 4 | 15,185,728 | 13,885 | 17,680,487 | 1,719 |
| 8 | 7,592,876 | 12,582 | 14,247,476 | 3,249 |
| **16** | **3,796,448** | **11,319** | **12,606,042** | **6,018** |
| 32 | 1,898,236 | 10,278 | 14,761,146 | 11,397 |
| 64 | 949,128 | 9,463 | 23,231,235 | 21,141 |
| 128 | 474,576 | 8,756 | 41,171,719 | 39,225 |

같은 저장 trace의 채택된 기존 전송 기록은 14,998 requests/21,016,558B다. 모델상
16-record가 요청과 bytes의 균형이 가장 좋지만, 두 숫자는 local-response causal model이며
실제 HF 지연·전체 P7P4 완료·다른 프로필의 레코드 폭을 증명하지 않는다. 재현 도구는
`scripts/benchmark/run-pc4-block-directory-model.mjs`이며 기존 trace와 파일을 수정하지
않는다.

모델만으로 레코드 경계를 가정하지 않도록 같은 도구의 `--verify-block 16` 경로에서
3,796,448B `GBLKIDX1` sidecar를 메모리에 구성하고 실제 qualified `graph.bin`을 블록별로
읽어 Hydra record 9,999개를 구조적으로 파싱했다. 공유 8MiB/2,048-entry LRU에서 실제
sidecar 1,638회/6,707,616B와 graph block 9,681회/5,898,426B가 발생해 모델과 정확히 같은
11,319회/12,606,042B가 나왔다. 각 레코드는 기존 GOFF exact 범위와 byte 대조했고 두
aggregate SHA-256이 모두
`bae61dc5c615a2fa68bfb8b9c915fd38040901f5f59ab25b10b552fb2004bbaf`로 일치했다. 비교용
exact read 9,999회/370,095B는 검증 oracle 비용이므로 후보 전송 수치에 포함하지 않는다.
이는 local adapter-format/record parity 증거이며 실제 네트워크 시간, 전체 탐색 결과,
다른 네 프로필의 자격을 닫지 않는다.

upstream 요청 후보는 `.safetensors` 변환이 아니라 프로필별
`graph block offsets v1`이다. body는 record ordinal `0, K, 2K, ... field_count`의 기존
`graph.bin` byte offset이며 마지막 값은 graph byte length다. manifest에는 K,
source GOFF/graph content identity, field count, target encoding, 완성 상태와 sidecar
content identity를 결박해야 한다. Jstris에서는 K=16을 실제 adapter/WAN A/B의 첫 후보로
삼는다. local adapter-format A/B는 위 9,999개 record에서 통과했지만 WAN A/B와 다른
킥 프로필의 qualified graph별 K·최대 block 계산은 남아 있다. 그 검증 전에는
muse918에게 파일 변경을 요청하지 않는다.

### 9.6 중앙 broker의 Tail 우선순위

Web의 계산/I/O 중첩 자체는 이미 원하는 비동기 구조다. 각 compact lookup은 응답을
기다리는 동안 다른 lookup과 ready CPU work를 진행하고, host는 첫 완료 응답부터
admission한다. 이를 CPU worker마다 독립 HTTP client를 두는 구조로 바꾸면 동일 span
결합, immutable identity별 in-flight dedup, byte 예약, 취소와 연결 pool이 분산된다.
따라서 바꿀 대상은 owner가 아니라 중앙 broker의 FIFO 선택 정책이다.
[libcurl의 connection-share 계약](https://curl.se/libcurl/c/CURLSHOPT_SHARE.html)도
HTTP/2·3 multiplex stream은 같은 multi/easy handle이 소유한 연결에만 추가되며 서로
다른 thread가 공유 연결에 stream을 붙이는 방식은 지원하지 않는다고 명시한다. native
CLI의 목표 역시 worker별 curl이 아니라 한 multi/pool owner여야 한다.

단일 FIFO에서는 여러 offset/index 요청 뒤에 나중에 생성된 exact graph 요청이 설 수
있다. graph 응답은 이미 resident credit을 점유한 작업을 풀지만, 새 index 응답은 아직
다음 graph 의존을 하나 더 만든다. `scripts/release/pc4/pc4-range-reader.mjs`는 direct
graph queue와 reusable/index queue를 분리하고, graph를 최대 3회 먼저 시작한 뒤 index
한 번을 강제하는 bounded-fair 3:1 정책으로 바꿨다. 물리 동시성 기본 4, 논리 slot 16,
64KiB 범위, 총 byte/request 예산은 바꾸지 않았다.

집중 JS 계약 24개와 Web host 계약 12개가 통과했다. 추가 계약은 (1) 오래된 index
backlog 뒤의 graph가 다음 전송 slot을 얻어 resident 작업을 풀고, (2) graph가 계속
생겨도 세 번 뒤에는 index가 반드시 진행하며, (3) 취소·shared budget·in-flight 결합을
그대로 보존함을 검사한다. 이는 실제 WAN tail 시간 개선량이나 물리 concurrency 4가
최적이라는 증거가 아니다. [exact-source 비게시 CI 35440710495](https://github.com/daejunnom/Clearra/actions/runs/35440710495)의
source, native CLI, surface, PC4 계약, preview-WASM 다섯 job은 모두 성공했다. 실제 대형
HTTP A/B는 여전히 남아 있다.

CLI/Discord의 당시 다음 구현 경계는 별도였다. 이후 App의 `pending_ranges()`와
`has_ready_work()`를 native host가 직접 소비하고, 최대 16개의 generation/profile-bound
demand를 한 persistent libcurl multi/pool owner에 넘기며 기본 4개의 transfer와 첫 완료
admission을 수행하도록 구현했다. HTTP/2를 지원하지 않는 환경도 같은 broker의 bounded
HTTP/1.1 연결을 사용한다. 요청별 curl thread 증대는 사용하지 않는다. 로컬 파일 driver와
합성 test adapter의 scalar 소비는 네트워크 경로가 아니며, 순차 graph dependency를
억지로 병렬화하지 않는다. 남은 것은 기본 제품 승격과 실제 큰 full-search A/B다.

### 9.7 실제 로컬 블록 A/B와 활성화 판정

모델의 요청 수만 보고 로컬 제품을 전환하지 않기 위해, 같은 검증 WASM
`3ceca8b51c9d6e54a837bc14fa5a6f17bbc8bcba`와 같은 immutable Jstris generation,
빈 필드/4L/P7P4/unique 입력에서 실제 local reader A/B를 추가했다. 모든 비교는 같은
read limit에서 demand SHA-256이 일치했고 완료 전에 취소됐으므로 전체 456,459개 완료
증거가 아니다. sidecar 생성 시간은 명시 다운로드/준비 비용으로 따로 기록해 검색
벽시계에서 제외했다.

먼저 sidecar 없이 GOFF에서 K=16 경계를 즉석으로 읽는 후보는 30,000 lookup에서
5.122초/20.96MB보다 느린 5.440초/31.64MB였다. graph block이 GOFF page를 공유 LRU에서
축출하므로 이 경로는 폐기했다. 실제 `GBLKIDX1` sidecar를 메모리에 만들어 읽은 경로는
다음 결과를 냈다.

| 경로 | lookup | 검색 벽시계 | 물리 읽기 | 파일 bytes | 준비 시간 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 기존 exact graph | 30,000 | 5.122초 | 15,026 | 20,959,851 | 0 |
| sidecar K=16 | 30,000 | 5.125초 | 9,683 | 5,906,618 | 0.152초 |
| sidecar K=4 | 30,000 | 5.018초 | 9,940 | 1,528,487 | 0.397초 |
| sidecar K=8 | 30,000 | 4.914초 | 9,840 | 3,019,568 | 0.234초 |
| 기존 exact graph | 100,000 | 16.440초 | 45,551 | 51,329,915 | 0 |
| sidecar K=8 | 100,000 | 16.667초 | 32,142 | 10,145,153 | 0.237초 |
| sidecar K=16 | 100,000 | 16.828초 | 31,005 | 19,551,146 | 0.159초 |

K=8의 30k 이득은 100k에서 재현되지 않았다. target ID 의미 검증을 기존 Rust
materializer에 남기고 JS는 record 경계만 구분하도록 줄인 뒤에도 100k sidecar는 인접한
exact baseline보다 느렸다. 따라서 **CLI/GUI의 명시 다운로드 제품 경로는 현재 exact
graph 읽기를 유지**한다. 실험 adapter와 `--graph-block-records` 계측은 남기되 caller가
명시하지 않으면 동작하지 않는다. 로컬에서 bytes 감소만으로 mmap/sidecar를 활성화하지
않으며, native mmap은 별도 positional-read A/B 전까지 계속 보류한다.

반대로 WAN에서는 10~30k건의 RTT와 Range header 비용이 로컬 파일 호출보다 훨씬 크므로
이 결과가 K=16 online 후보를 기각하지는 않는다. 온라인 채택 조건은 실제 upstream
sidecar와 동일한 중앙 broker에서 exact 대비 전체 벽시계·tail·429를 비교하는 것이다.
정확한 요청 형식은
[upstream graph-block sidecar 요청안](pc4-upstream-graph-block-sidecar-request-2026-09-19.md)에
분리했다. `.safetensors` 변환이나 graph 재정렬은 요구하지 않는다.

### 9.8 native 유한 batch 후보와 최종 multi-owner 경계

기존 native CLI는 App이 독립 lookup을 여러 개 보유해도 `pending_range()` 하나를
동기 `curl_range`로 처리했다. 각 Range마다 새 curl process와 연결 수립 경계가 생기고,
그 응답이 끝날 때까지 같은 host에서 ready CPU work도 진행하지 못했다. 이 정책은
요청 순서와 실패 지점이 단순하지만 WAN RTT가 lookup 수에 거의 직렬로 더해진다.

후보 `483f260`은 이 경계를 다음처럼 바꾼다.

- App의 최대 8개 독립 lookup과 `has_ready_work()`는 그대로 둔다. 이는 CPU worker
  수가 아니라 동시에 보유하는 qualified continuation 수다.
- 이미 cache에 있는 요청을 먼저 admission하고, 최대 16개 known graph demand를
  4KiB gap/64KiB physical span 안에서 결합한다.
- 한 `curl --parallel --parallel-max 4` process가 한 유한 batch를 소유한다. Range를
  시작하기 전에 request/byte credit 전체를 예약하며, body는 transfer별 격리 파일에
  쓰고 stdout에는 index/status/Content-Range receipt만 둔다.
- 먼저 끝난 transfer부터 exact lookup/request ID로 admission한다. 그 사이 host는
  ready CPU work를 계속 진행한다. HTTP 200/429/416, 잘못된 Content-Range, oversized,
  truncated body는 body 파일을 정상 결과로 읽기 전에 서로 다른 오류로 분류한다.
- shell, `.curlrc`, credential, 자동 retry, HTTP/3 강제는 사용하지 않는다. 동일한
  immutable revision/artifact identity와 기존 취소·전체성 계약을 유지한다.

첫 exact-source CI는 source/surface와 기존 PC4 Core/App 계약을 통과한 뒤 새 CLI 파일의
`Vec<Transfer>` 타입 추론 오류로 native CLI/PC4 CLI 단계가 실패했다. 따라서 이 실행은
성능 또는 실행 성공 증거가 아니다. `17821a2`에서 타입을 명시하고 command 구성 및
HTTP receipt 분류 계약을 추가했다. 후속 exact-source 비게시 CI
[35444261101](https://github.com/daejunnom/Clearra/actions/runs/35444261101)의 source,
native CLI, surface, PC4 계약 job은 모두 성공했다. preview-WASM은 Rust/WASM 입력이
없어 의도적으로 생략됐다. 이는 유한 native batch의 컴파일·계약 증거이며 persistent
multi, Cloud image 또는 전체 검색 성능 증거는 아니다.

이 후보는 제안한 최종 정책의 **중간 단계**다. 유한 curl process에는 실행 중 새 easy
handle을 추가할 수 없으므로 batch의 세 요청이 끝나고 하나가 오래 남았을 때, 먼저 끝난
응답이 만든 새 Range를 같은 연결의 빈 slot에 즉시 넣지 못한다. 또 현재 native 후보는
재사용 index page 정책을 보존하기 위해 graph batch만 병렬화하고 index/offset은 기존
scalar reader를 사용한다. 즉 다음 두 tail은 아직 남는다.

1. 한 유한 batch 안의 마지막 느린 transfer가 다음 batch 생성을 막는 process tail
2. graph와 index 요청이 섞였을 때 index scalar 왕복이 남기는 dependency tail

같은 Windows HTTP/1.1 curl에서 전송 경계만 작은 실 A/B로 확인했다. exact Jstris
revision의 `graph.bin`에서 서로 떨어진 4KiB Range 네 개를 사용했고, 먼저 두 경로를
한 번씩 warm-up한 뒤 순서를 교차했다. 제품 App/해법 탐색은 실행하지 않았다.

| 전송 경계 | 세 번의 벽시계 | 중앙값 |
| --- | --- | ---: |
| 기존과 같은 curl process 4회 직렬 | 4.252s, 4.275s, 4.257s | 4.257s |
| curl process 1회, parallel max 4 | 1.300s, 1.321s, 1.278s | 1.300s |

이 제한된 표본의 중앙값은 약 3.27배 차이지만, process/TLS/RTT를 함께 비교한
transport-only 수치이며 전체 검색 가속률이 아니다. 같은 host에서
`--parallel-immediate`는 기본 parallel 중앙값 1.285s 대비 1.088s로 약 15% 빨랐다.
그러나 이 host에는 HTTP/2가 없어 다중 연결 시작 지연만 비교했다. HTTP/2 환경에서
`--parallel-immediate`는 multiplex를 기다리는 대신 연결을 더 열 수 있으므로 Cloud
exact-image A/B 전에는 제품 기본값으로 넣지 않는다.

별도 단일 HTTP/2 client/한 서버 연결 실험에서 네 4KiB stream의 첫 wave는 1.990초,
같은 client를 재사용한 후속 네 wave는 0.832~0.852초였다. 같은 object/Range를 반복해
CDN cache와 연결 재사용을 분리할 수 없는 제한된 결과지만, 유한 curl process를 매
wave 종료하는 구조를 최종안으로 삼지 않을 근거에는 부합한다. 최종 A/B는 서로 다른
실제 demand trace와 connection/TLS counter를 기록해야 한다.

최종 native owner는 OS 계산 thread마다 HTTP client를 두지 않는다. 한 host-lifetime
libcurl multi/pool owner가 logical queue를 소유하고, physical slot 기본 4개 중 하나가
비는 즉시 새 easy handle을 추가해야 한다. libcurl multi는 진행 중에도 handle 추가가
가능하고 완료별 message를 제공한다. graph direct와 reusable index queue는 Web과 같은
bounded-fair 3:1을 사용한다. HTTP/2·3 multiplex stream은 같은 multi/easy owner의
connection에만 추가할 수 있으므로 worker별 client는 오히려 연결 재사용, in-flight
dedup, cache와 byte credit을 분산시킨다.

연산이 HTTP보다 빠른 경우에도 CPU가 무제한 요청을 생산하지 않는다. concurrent lookup
8, logical queue 16, physical transfer 4, 64KiB/span, 64MiB/request budget과 union의
resident/waiting byte budget이 backpressure를 제공한다. 반대로 CPU가 느리면 broker는
빈 slot을 허용하며 불필요한 speculative Range를 만들지 않는다. tail에서는 batch 크기를
억지로 채우지 않고 ready demand를 즉시 시작한다. 성능 판정에는 평균만 쓰지 않고
slot-idle time, queue wait p50/p95/p99, first/last completion, connection/TLS count,
HTTP version, 429와 transferred bytes를 함께 기록한다.

### 9.9 HTTP/2·HTTP/3, metadata cache, safetensors와 mmap 판정

curl의 공식 multi 계약은 한 thread에서 다수 전송을 동시에 진행하고, 진행 중인 multi에
handle을 추가하며, 완료별로 회수할 수 있게 한다. HTTPS curl은 빌드가 지원하면 ALPN으로
HTTP/2를 기본 협상한다. 그러나 protocol은 소스 옵션이 아니라 **실행 curl의 feature**다.

- 현재 Windows `curl 8.21.0 (Schannel)`의 feature 목록에는 HTTP2와 HTTP3가 모두 없다.
  따라서 이 환경의 유한 batch는 HTTP/1.1 최대 네 연결로만 검증한다.
- endpoint 능력은 별도로 확인했다. 같은 exact revision/네 4KiB Range를 .NET 단일
  `HttpClient`, `MaxConnectionsPerServer=1`, HTTP/2 우선으로 동시에 전송했을 때 네
  응답이 모두 HTTP 206, version 2.0, exact Content-Range/4096B였고 전체 벽시계는
  1.299초였다. 이는 HF/CDN이 이 요청에서 HTTP/2 다중 Range stream을 처리한다는
  bounded transport 증거이며 product curl·전체 검색의 협상 증거는 아니다.
- production의 `node:22-bookworm-slim` 경로는 accepted runtime Dockerfile에 curl을
  설치하지 않았으므로 후보 branch에서 curl/CA를 명시 설치하고 build 중 HTTPS protocol과
  HTTP2 feature를 검사하도록 수정했다. Debian Bookworm `libcurl4`는 `libnghttp2-14`에
  의존하지만, exact image build와 실제 `%{http_version}` receipt를 확인하기 전에는
  Cloud 협상 성공을 주장하지 않는다.
- Bookworm 기본 패키지에서 HTTP/3를 가정하지 않는다. curl의 HTTP/3는 QUIC backend가
  들어간 별도 build가 필요하고 proxy에서는 제약이 있다. 이 workload는 작은 immutable
  Range의 연결 재사용이 핵심이므로 우선순위는 persistent HTTP/2 multi이며, HTTP/3는
  같은 A/B에서 handshake/packet-loss tail이 실제로 줄 때만 opt-in한다.

`.safetensors`는 지금 요청할 upstream 변경이 아니다. 이 포맷의 작은 JSON header는
named dense tensor의 dtype/shape/data offset을 찾는 데 유리하지만, PC4 `graph.bin`은
GOFF가 구분하는 가변 길이 Hydra record다. 세 flat artifact는 이미 manifest에 byte
length/content identity/role을 갖고 있어 safetensors header를 추가해도 graph record
주소를 얻으려면 GOFF 또는 별도 block directory가 여전히 필요하다. 오히려 최초 header
길이와 JSON을 위한 Range 두 번, 변환본 generation/identity, 다섯 프로필 독립 활성화
복잡도가 늘어난다. upstream 요청은 기존 graph를 재포장하는 safetensors보다 source
GOFF/graph identity에 결박된 작은 `graph block offsets v1` sidecar를 유지한다.

metadata cache는 별도이며 우선순위가 높다. 캐시 가능한 것은 `(repository, resolved
revision, profile, 세 artifact path/size/content identity)`에 결박된 qualified header,
field count와 block-sidecar header다. `main` 이름 자체를 신뢰해 영구 캐시하지 않고,
작업 시작 시 revision discovery를 갱신한 뒤 같은 exact revision의 검증 완료 metadata만
재사용한다. body/Range admission은 여전히 exact Content-Range와 snapshot identity를
검사한다. CLI의 프로세스 간 cache와 Discord의 host-lifetime cache는 서로 다른 owner로
두며, cache miss/corruption은 원격 재검증이지 오프라인 탐색 허가가 아니다.

`mmap`은 명시 다운로드한 로컬 파일에만 적용 가능한 후보다. HTTP 왕복을 줄이지 않으며
Web/OPFS에도 그대로 적용할 수 없다. 현재 local reader는 index 4KiB page cache와 graph
exact positional read를 쓰고, 저장된 큰 입력 prefix에서 전체 local-reader wait를 0으로
가정해도 조건부 상한은 약 1.16~1.18배였다. mapping은 활성 generation lease가 파일
교체/절단을 막는 동안 read-only로 만들고, Windows/Linux cold/warm cache, page fault,
RSS와 전체 wall time을 기존 positional read와 A/B한 뒤에만 채택한다. 이 결과 없이
`memmap2` 의존성을 제품에 추가하거나 local default를 바꾸지 않는다.

### 9.10 opt-in persistent libcurl 후보

유한 process wave의 tail을 코드 수준에서 분리해 측정하기 위해
`native-pc4-libcurl` feature를 추가한다. 기본 feature는 계속
`online-pc4-tablebase`뿐이며 이 후보는 제품 기본 경로, 배포 산출물 또는 자격 증거가
아니다. 후보는 한 native 온라인 실행 동안 하나의 `curl::multi::Multi`를 소유하고 다음
경계를 지킨다.

- logical demand는 최대 16개, active easy handle과 HTTP/2 stream 상한은 4개다.
- CPU/App의 ready work를 먼저 진행하고, 이미 실행 중인 transfer가 있어도 새로 드러난
  exact graph demand를 queue에 넣는다.
- 완료 message를 exact `(lookup_session, request_id, artifact identity, offset, length)`에
  대조해 admission한 즉시 빈 active slot을 채운다. 다음 wave 전체를 기다리지 않는다.
- 기존 4KiB gap/64KiB span 결합, 64MiB/100,000 request 예산, HTTPS final URL,
  HTTP 206/Content-Range/길이 검증과 취소 시 multi owner 폐기를 유지한다.
- 연결은 easy handle별이 아니라 같은 multi handle의 cache를 공유한다. 빌드가 HTTP/2를
  지원하면 TLS에서 HTTP/2를 우선하고, 그렇지 않으면 같은 네 slot의 bounded HTTP/1.1
  fallback 의미를 유지한다. HTTP/3는 이 feature에 포함하지 않는다.

첫 후보의 후속 수정은 generation qualification, FHID/GOFF scalar cache miss와 graph
transfer를 **한 multi owner**로 합쳤다. qualification의 첫 bounded Range가
DNS/TCP/TLS/ALPN을 먼저 지불하고, 같은 owner의 connection cache를 실제 graph와 index가
재사용한다. graph가 진행 중이거나 CPU ready work가 있을 때 scalar 동기 read를 시작하지
않아 기존 graph-first 순서를 유지한다. 이는 owner 경계를 닫은 것이지 mixed graph/index
3:1 fairness와 full-search tail을 측정 완료했다는 뜻은 아니다. Ubuntu/Windows 비게시
matrix는 feature의 compile/link와 callback 계약만 검사하며, 이어지는 실제 full-search
A/B가 다음을 동시에 만족하기 전에는 기본 승격하지 않는다.

1. exact solution/count/digest와 오류 분류가 기본 유한 batch와 같다.
2. 서로 다른 실제 demand trace에서 first result, total wall, 마지막 10% tail이 악화되지
   않는다.
3. queue wait p50/p95/p99, slot idle, connection/TLS count, negotiated HTTP version,
   transferred bytes와 429를 함께 기록한다.
4. index를 같은 owner로 옮길 때 graph 3회 뒤 index 1회를 보장하고, index page cache와
   graph in-flight dedup을 보존한다.

`reqwest`/별도 async runtime은 이 첫 A/B에서 선택하지 않는다. CLI 기본 의존 그래프와
runtime owner를 크게 늘리지 않고 libcurl multi의 진행 중 handle 추가·connection cache
계약을 직접 검증하기 위함이다. 반대로 이 후보가 두 release target에서 링크 비용이나
성능 회귀를 보이면 feature를 제거하고 기존 외부 curl batch를 유지한다.

exact source `69c222b94322363bb54b09270da1595e1379b378`의 비게시
[CI 35446511211](https://github.com/daejunnom/Clearra/actions/runs/35446511211)은 source,
PC4 contracts, native CLI, surface contracts와 Ubuntu/Windows 후보 matrix가 모두
성공했다. 후보 unit은 final redirect block의 bounded body/Content-Range 수집과 in-flight
exact-ID dedup/identity-change 거부 2개이며 두 플랫폼 모두 경고 없이 통과했다. clean
candidate build는 Ubuntu 1분 35초, Windows 4분 49초였다. 이는 compile/link와 순수 owner
계약 증거이지 live HF 협상, finite batch 대비 전체 검색 성능, 다섯 프로필 자격 또는
Jstris 빈 필드 P7P4 456,459개 완료 증거가 아니다. Windows 비용과 아직 남은 mixed-index
tail 때문에 feature는 opt-in 상태를 유지한다.

### 9.11 HTTPS prewarm 수명, Keep-Alive/TFO와 Setup 후보 주입 경계

#### 연결 준비와 수명

HTTPS 병목은 (1) revision discovery, (2) DNS/TCP/TLS/ALPN, (3) FHID/GOFF 의존
왕복, (4) graph transfer와 마지막 tail로 나눈다. 이번 수정은 2번의 중복을 줄이지만
1·3·4가 사라졌다는 증거는 아니다.

- native opt-in 경로는 자격 확인용 Range부터 검색 중 index/graph Range까지 한
  [`curl::multi::Multi`](https://curl.se/libcurl/c/libcurl-multi.html)를 유지한다. logical queue 16/active 4 제한과 exact receipt 검증은
  그대로다. [`MAXAGE_CONN=300s`](https://curl.se/libcurl/c/CURLOPT_MAXAGE_CONN.html)는 너무 오래 idle한 연결을 재사용하지 않는 상한이고,
  [TCP keepalive](https://curl.se/libcurl/c/CURLOPT_TCP_KEEPALIVE.html) 60s/30s는 죽은 peer를 발견하기 위한 probe다. 둘 다 요청 heartbeat나
  무기한 연결 보장이 아니다.
- Web은 사용자가 TB를 켜는 순간 generation discovery·header qualification을 WASM
  컴파일 및 worker warmup과 겹쳐 시작한다. WASM capability 검사는 두 준비 경로가 합류한
  뒤에 별도로 수행한다. 실행 중인 다른 검색이 있더라도 상위 controller가 transport-only
  prewarm 메시지를 즉시 보내며, worker는 활성 검색의 WASM table을 건드리지 않고
  discovery/handshake만 먼저 시작한다. 이 메시지는 runtime-prewarm 완료를 가장하지 않으며
  전체 runtime 재설정은 현재 검색 종료 뒤로 미룬다. TB를 다시 꺼도 immutable generation asset과 진행 중 warmup은
  worker disposal/fail-close 전까지 유지한다. 따라서 재활성화는 같은 worker cache를
  재사용하지만, 브라우저의 HTTP connection pool/socket 만료는 애플리케이션이 소유하지
  않는다. 마지막 온라인 접촉에서 30초 이상 지난 뒤 재활성화하면 검색보다 먼저 자격된
  artifact에 정확히 1바이트/1요청만 허용한 Range를 보내 전송 경로를 다시 준비한다. 이
  요청은 generation 자격을 새로 만들지 않고 실패 시 해당 온라인 실행만 typed unavailable로
  닫는다. 꺼진 동안 dummy Range를 주기적으로 보내는 heartbeat나 timer는 데이터 사용량·
  rate limit·백그라운드 throttling 때문에 만들지 않는다.
- live A/B에서는 각 transfer의
  [`CURLINFO_NUM_CONNECTS`](https://curl.se/libcurl/c/CURLINFO_NUM_CONNECTS.html), redirect count, negotiated HTTP
  version, name lookup/connect/TLS/start-transfer time을 기록한다. 첫 qualification 뒤 실제
  graph wave에서 새 connection 수가 0인지가 handshake 재사용 판정이며, warm cache의 총
  시간만으로 판정하지 않는다.

동일한 upstream revision `ea61380b31fa3dc9ffb4c8505c9a09c1b421ef31`에 대해 비게시
live job을 한 번씩 실행했다. workflow의 나머지 중복 job은 측정 job 성공 뒤 취소했으므로
아래 링크의 **live job 성공**만 증거이며 전체 workflow 성공을 뜻하지 않는다.

| 구현/source | qualification 물리 transfer | qualification 누적 transfer time | 후속 graph | 전체 test wall |
| --- | ---: | ---: | --- | ---: |
| scalar `87c2cdd`, [job 105920505792](https://github.com/daejunnom/Clearra/actions/runs/35451928963/job/105920505792) | 20 | 4,227.487ms | 새 연결 0, 재사용 1, HTTP/2, 181.021ms | 4.77s |
| 3단계 batch `cb65fe6`, [job 105922367034](https://github.com/daejunnom/Clearra/actions/runs/35452635266/job/105922367034) | 15 | 3,960.070ms | 새 연결 0, 재사용 1, HTTP/2, 125.145ms | 1.89s |

두 실행 모두 qualification에서 origin/CDN용 새 연결 2개만 만들고 후속 graph에서는 새
연결을 만들지 않았다. 단계 내부 multiplex와 인접 범위 결합으로 test wall은 이 한 표본에서
약 60.4% 줄고 transfer는 25% 줄었다. 누적 transfer time은 동시에 열린 stream 시간을
합한 값이므로 wall time처럼 해석하지 않는다. 이 결과는 최초 qualification RTT 병목을 크게
완화하고 qualification-to-graph handshake 재사용을 입증하지만, P7P4 전체 검색의 mixed
index/graph 의존 사슬·429·마지막 tail이 해결됐다는 증거는 아니다.

[`TCP Fast Open`](https://curl.se/libcurl/c/CURLOPT_TCP_FASTOPEN.html)은 기본으로 켜지 않는다. 최초 TCP handshake에서 최대 한 RTT를 줄일 수
있지만 libcurl 계약상 TFO를 켜면 TLS session cache가 작동하지 않고 일부 네트워크에서
문제가 알려져 있다. 이번 workload는 한 번의 cold connect보다 같은 TLS/HTTP/2 connection
재사용이 더 중요하고 Rust `curl` wrapper에도 안정적인 portable option이 노출되어 있지
않다. 필요하면 Linux 전용 local-only A/B로 분리하되 결과가 connection reuse보다 우월한
경우에만 다시 제안한다. HTTP/3 역시 Bookworm/브라우저가 실제 QUIC를 협상했다는 receipt와
packet-loss tail 이득이 생기기 전에는 후보 우선순위를 올리지 않는다.

따라서 이 시점의 판정은 **HTTPS owner/handshake 중복 결함은 제거했고 qualification
왕복은 실측상 크게 완화했지만 전체 검색 병목 해결은 미증명**이다. 제품 기본 native
경로는 여전히 유한 외부 curl batch이고, `native-pc4-libcurl`은 full-search A/B 전까지
opt-in이다.

Web 재-prewarm은 이후 작은 `field_hash_to_id`가 아니라 선택 가능한 ready profile의 실제
`graph*.bin` 첫 1바이트를 읽도록 좁혔다. 작은 index와 대형 LFS graph가 서로 다른 HTTP
cache/CDN 경로를 사용할 수 있기 때문이다. 이 touch만 `Request.cache=no-store`를 사용해
브라우저 cache hit를 live handshake로 세지 않으며, 일반 탐색 Range의 immutable cache
정책은 바꾸지 않는다. 호출/전송 상한은 여전히 1회/1바이트이고 timer heartbeat는 없다.
전송 및 worker 수명 계약 63개와 contract TypeScript 검사가 통과했다. 이는 graph data-path
prewarm의 소스·계약 증거이며 브라우저 socket 영속성이나 full-search 완료 시간 증거는 아니다.

#### Setup 탐색 적용

새 실행 경계는 App에서 exact `SetupSearch` differential proof를 통과한 complete PC4
candidate family만 받는다. Core는 모든 후보를 active ILC catalog, 초기 필드, 4L 깊이,
piece multiset과 다시 대조한 뒤 기존 Setup graph의 shape·spin·score·BuildUp·probability
평가를 그대로 실행한다. partial/miss/rate limit/profile 또는 generation 불일치는 Setup
불가능 증명이 아니며 기존 명시 fallback owner로 돌려보낸다. 주입 경로 결과는 일반 Setup
graph cache에 섞지 않는다.

현재 구현은 root completion family를 미리 공급해 최초 exact geometry compile을 생략하고,
후속 residual은 기존 exact compiler가 책임지는 보수적 기반이다. 즉 완결 후보를 받았다는
이유만으로 임의의 row 조합을 허용하지 않으며, 대규모 residual index나 후보 전체 스캔을
추가하지 않는다. 실 profile별 SetupSearch 자격과 differential proof가 아직 없으므로 제품
capability는 계속 꺼져 있다.

과거 기록에는 다음 Setup 수치가 있지만 어느 것도 동일 입력의 HF TB on/off A/B가 아니다.

- IOTS, setup piece 최대 1: 29.1초, 77 setups
- IOTS, 최대 2 locks: 약 35.68초, 330MiB, 1,701 setups/610,196 families
- 과거 broad fixture: 1,519.58초; 후속 routing fixture 71.35초

따라서 기존 기록을 TB 가속률로 재사용하지 않는다. 새 A/B는 동일 query/profile/kick/
generation/worker/memory/time budget에서 offline과 TB가 모두 완료될 때만 wall time,
first-result, geometry nodes, peak RSS와 exact result digest를 수치 비교한다. offline이 고정된
CPU·메모리·시간 예산 안에 완료되지 않아 같은 입력의 숫자 비교가 불가능하되, 자격된 TB가
complete candidate family를 공급하고 기존 Setup evaluator가 exact digest와 함께 완료하면
이를 **feasibility dominance(연산 가능성 우위)**로 기록한다. 이를 무한 배속이나 일반적
성능 수치로 표현하지 않는다. 이번 bounded synthetic 검사는 주입 family와 offline exact
geometry의 root row 집합 동등성만 확인하며 실제 HF/대형 Setup A/B를 대체하지 않는다.

새 Setup receipt는 `source SHA / query digest / profile / immutable generation / worker 수 /
CPU·memory·time budget / offline terminal state / TB terminal state / exact result digest /
candidate·coverage count / first-result·wall·nodes·peak RSS`를 한 묶음으로 남긴다. 양쪽 완료
receipt가 있으면 A/B, offline이 동일 고정 예산에서 typed resource-limit 또는 timeout으로
끝나고 TB만 완료하면 feasibility dominance다. 과거의 서로 다른 IOTS/broad fixture나
사용자 중단을 offline 실패 receipt로 소급 변환하지 않는다.

2026-09-20 구현에서는 이 경계를 실행 코드와 공개 표면에도 고정했다.

- host generation의 `pc_search_target_lines`와 `setup_search_target_lines`를 분리했다.
  upstream 선언과 bounded reader 표본만 통과한 현재 Jstris 세대는 reader-ready일 뿐
  exact target-qualified가 아니므로 두 목록 모두 `[]`다. 13개 omitted transition을 포함한
  exact outgoing-edge/known-answer/offline-parity receipt가 추가될 때 PC Search 4L만
  독립적으로 `[4]`가 될 수 있고, 그 자격을 Setup 자격으로 빌려 쓸 수 없다.
- Web PC/Setup control은 아직 generation을 읽지 않은 상태에서도 사용자가 opt-in할 수 있다.
  opt-in이 먼저 transport prewarm을 시작하고, loading 동안에는 unavailable로 오표시하지
  않는다. qualification이 끝난 뒤 선택한 profile의 해당 target 목록에 목표가 없으면 실행
  버튼을 막고 unavailable을 표시한다. 따라서 최초 handshake를 시작하려면 이미 자격 정보가
  필요했던 순환 의존은 없다. CLI의 명시 요청은 `setup_pc_acceleration_not_qualified`로
  fail-closed되고 offline을 자동 시작하지 않는다.
- `scripts/release/pc4/setup-ab-receipt.mjs`는 위 필드를 exact-key로 검증한다. 양쪽 완료의
  digest/candidate/coverage가 모두 같을 때만 `exact-parity`와 activation evidence가 된다.
  offline timeout/resource-limit와 TB 완료 조합은 `feasibility-dominance`일 뿐 activation
  evidence가 아니다. online receipt는 compute 전 prewarm을 필수로 하고, local receipt는
  HTTP 요청·연결·전송량을 모두 0으로 강제한다.

이는 Setup evaluator에 complete candidate family를 주입하는 기존 실행 기반을 공개 제품
capability와 연결하기 위한 준비다. 비게시
[Integration Contracts run 35457548147](https://github.com/daejunnom/Clearra/actions/runs/35457548147)의
독립 exact oracle은 13개 upstream-omitted 전이를 모두 dead로 분류했다(live 0,
unknown 0, 8.89초). 다만 이는 generation 전체의 outgoing-edge/known-answer/offline-parity
identity나 Setup differential receipt를 만들지 않는다. 동일 입력 Setup A/B도 없으므로,
현재 세대에서 SetupSearch를 켜거나 과거 수치를 속도 향상 근거로 사용하지 않는다.

#### 2026-09-20 재감사 결론

연결 준비와 요청 병렬성은 다시 소스·계약 테스트로 대조했다. Web의 한 bounded reader는
독립 Range를 최대 4개까지 즉시 시작하고 먼저 끝난 응답부터 exact lookup ID로 Rust에
admit한다. native opt-in 경로도 qualification과 검색이 같은 multi connection pool을
소유한다. 따라서 worker마다 별도 HTTP client를 만들거나 한 batch의 최후 응답까지 CPU를
막는 구조로 되돌아가지 않는다. 브라우저 `fetch`의 `keepalive` 옵션은 socket 유지 옵션이
아니라 문서 unload 뒤 요청 생존 옵션이므로 여기에 사용하지 않는다. TCP keepalive는 native
dead-peer 검출로만 유지하고, TFO는 TLS session cache 손실과 네트워크 호환성 위험 때문에
기본 off를 유지한다.

UI부터 worker까지의 호출 경계도 다시 추적했다. 실행 중 `prewarm()`을 모두 보류하던 상위
controller를 수정해 **TB off→on 전이만** 활성 worker에 즉시 전달하고, off 전이와 worker
pool 재설정은 기존처럼 terminal 뒤에 수행한다. worker 내부의 active-job branch와 상위
controller 양쪽에 회귀 계약을 두었다. generation discovery 전 체크박스를 비활성화하던
순환도 제거했으며, exact target 자격이 없는 generation은 탐색 실행만 계속 fail-closed한다.
이 변경은 handshake 시작 시점을 앞당기지만 브라우저 socket 수명을 보장하거나 full-search
tail 해결을 주장하지 않는다.

전송 실패는 이제 HTTP 429의 bounded numeric `Retry-After`, offline, timeout, 일시적인
408/425/5xx unavailable을 WASM/App의 typed Range failure로 전달한다. 이 경계는 offline
탐색을 자동 시작하지 않는다. 잘못된 Content-Range, whole-body 200, immutable identity
불일치는 데이터/프로토콜 오류로 계속 분리한다. 이것은 실패 의미 보존의 완료이지,
full-search tail이나 HF rate limit 자체를 제거했다는 증거는 아니다.

제품 오류 투영도 같은 타입을 보존한다. Web은 miss/offline/rate-limit/timeout/
unavailable을 서로 다른 공개 오류로 분리하고 CLI는 내부 `pc4_online_*` reason 대신
언어별 안내를 출력한다. 두 surface 모두 자동 offline fallback을 시작하지 않으며,
사용자가 TB를 끄거나 `--no-tablebase`로 다시 실행해야 offline 계산이 시작된다.
GUI Stop과 CLI Ctrl+C가 그 명시 실행의 중단 경계다.

Setup은 기존 수치가 동일한 TB on/off 입력이 아니므로 재사용 A/B를 만들지 않았다. 현재
exact target receipt가 없어 실제 HF Setup arm을 실행하면 안 된다. 이후 동일 입력에서 두
arm이 모두 완료될 때만 수치 A/B를 남기고, offline이 고정 자원에서 typed timeout 또는
resource-limit으로 끝나지만 target-qualified TB arm이 exact 완료한 최초 기록은 수치 배속이
아닌 `feasibility-dominance`로만 분류한다.

Setup 실행 ingress는 empty 10×4/4L과 한 compiled condition으로 제한해 연결했다. 명시 TB
요청이 snapshot 없이 WASM에 들어오면 이제 `pc4_online_generation_unavailable`로 닫히며
오프라인 Setup으로 조용히 하강하지 않는다. 현재 generation의
`setup_search_target_lines`가 비어 있으므로 실제 HF arm 또는 feasibility-dominance receipt는
아직 만들 수 없다. 첫 기록은 동일 입력·고정 자원에서 offline typed timeout/resource-limit과
target-qualified TB exact completion이 동시에 남는 시점에만 생성한다.

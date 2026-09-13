# PC4 비동기 탐색 큐·HTTP 전송·파일 배치 검토

작성: 2026-09-14. 소스 기준: `6172f58eaa6c8e0e7cbde902e88a1077ec242d9f`,
`codex/v0.9.0-stacked-on-v0.8.1-20260912` 작업 트리.

이 문서는 사용자 제안과 현재 소스/실제 전송을 비교한 **미적용 설계 후보**다.
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

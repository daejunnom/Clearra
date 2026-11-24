# P7P4 초기 최적화 복원과 공유 커버리지 수명 개선

작성일: 2026-11-24, Asia/Seoul. 최종 측정 소스는
`853305ad00e0d873c05b0e89fd1c419deb1815de`이다.

## 1. 완료한 작업과 적용 범위

사용자가 제공한 초기 작업 기록을 현재 소스·원격 이력과 대조하고, 재사용 가능한
후보별 의존성 검증과 가지치기를 복원했다. 이후 실제 제품 compiler의 공급 증명
발동 조건을 수정하고, Summary에서 전체 입력 커버리지가 완성되면 다른 worker도
불필요한 full coverage 자료를 해제하도록 개선했다.

최종 P7P4 일반 실행에서 Summary는 23.48/23.79초, Trace는 32.38/34.67초에
완료했다. 같은 Trace 출력 계약의 이전 측정은 148.34/147.90초였다. 새 공유
완료 기능만 끈 동일 바이너리 Summary는 39.94/35.48초였다. 아래에서 출력 정책,
후보 증거, 진단 여부와 구현 차이를 구분한다.

geometry family를 직접 푸는 DAG solver는 사용하지 않았다. 원래 기하 후보를
열거하고 각 후보의 배치 의존성·물리 이동·공급 교집합을 검증한다. Full 6L,
CI 변경·실행, main 승격 및 배포는 이번 작업 범위에서 계속 보류한다.

| 커밋 | 변경 |
|---|---|
| `7d04c8c942feff703e14c1d44fd4b2e6937c88b2` | 후보별 의존성 product, ILC·공급 prefix, ordered kick 가지치기, 원본 ordinal·EOF 보정, coverage 소유권 복원 |
| `e10330274b2c011354c40b781eb1d13eb873d4b8` | 실제 compiler의 요청 provenance를 유지하면서 FullQueueOracle 공급 증명을 허용 |
| `853305ad00e0d873c05b0e89fd1c419deb1815de` | 동일 요청의 exact shared completion으로 Summary worker의 full coverage 저장 공간 해제 |

측정 바이너리 SHA-256:
`cf15c97ec3ae697b27d0379035749832bb4563301270d7a6315ac716c93cfda4`.
문서만 추가한 후속 커밋은 이 측정 소스와 구분한다.

## 2. 초기 기록과 원격 이력에서 확인한 사실

첨부 기록은 `8d3a117`의 평균 83.17초에서 `f894cc9`의 평균 38.62초로 개선했다고
보고하며, 출력은 **Summary/CountUnique**였다. 현재 확보된 150초대 원문은
**Trace/CountUnique**이다. 두 시간을 곧바로 같은 요청의 회귀 배수로 비교할 수 없다.

첨부가 가리키는 `f894cc9` 등의 원본 Git object, 당시 바이너리와 원시 실행 receipt는
확보하지 못했다. 따라서 이번 작업은 초기 기록의 원리와 현존 코드의 선택적
복원이며, 과거 소스의 byte 단위 복구나 과거 38.62초의 재현이라고 주장하지 않는다.

원격을 다시 fetch하여 `codex/build-elision-cloud-boundary-20261123`의
`4a7d4e3a9edb39ad8ac68df769717faf03807938`이 유지됨을 확인했다. 전체 ref의
커밋 1,548개는 메타데이터와 변경 경로를 조사했고, 관련 ordinary hot path의
실제 diff를 검토했다. 1,548개 커밋의 모든 변경 줄을 수작업 검토했다는 뜻은 아니다.

직접 family solver 도입 직전 `cf8af101`과 원격 최신 `4a7d4e3a`를 해당 기능 OFF로
각각 실행한 결과는 71.676743초와 71.988743초였다. 둘은 같은
`physical-prefix-filtered` 후보 계약이며 각 한 번의 관측이다. 큰 ordinary 회귀를
뒷받침하지 않지만, 모든 옵션의 무회귀나 두 구현의 통계적 동등성을 증명하지도 않는다.
이 두 실행은 1,695,057개 후보를 내보내며 raw 후보 digest를 계산하지 않으므로,
아래 raw 후보 전체를 보존하는 실행과 속도 비율을 계산하지 않는다.

직접 family 추가 이후 확인한 worker·coverage·language owner와 일반
geometry/reachability의 변경은 해당 feature에 격리되어 있었다. 이전 측정 소스
`5c14a657`은 최초 직접 family 도입 커밋 `85bce9bd`의 후손도 아니다. 이번에 발견한
실제 문제는 일반 경로의 최적화 연결 누락, 공급 증명 조건, 자료의 해제 시점이다.

이력 근거는 증거 묶음의 `complete-commit-inventory.v1.*`,
`shared-off-path-history-review.md`, `ordinary-geometry-shared-history-reviewed.md`,
`initial-context-structural-restoration-audit.md`에 남겼다.

## 3. 측정 계약과 결과 검증

모든 최종 비교는 빈 10×4 보드, Jstris180, fresh Standard7Bag 11 draws,
10 placements, 빈 Hold 허용, CountUnique, FullQueueOracle을 사용했다.
성능 실행은 Auto로 논리 CPU 9개를 사용했고 worker 수를 줄이지 않았다.
테스트 실행기의 단일 test thread 설정은 성능 실행 설정과 별개다.
이는 native release 실행의 계측이며, 브라우저 WASM이나 GUI 클릭부터 화면 표시까지의
전체 지연을 다시 측정한 값은 아니다.

| 검증 항목 | 전체 raw 후보 계약의 공통 값 |
|---|---|
| 원본 입력 수 | 4,233,600 |
| 원시 기하 후보 수 | 29,856,840 |
| 원시 후보 digest | `d6715a89054ef642` — 실제 계산 |
| 정렬된 최종 identity 수 | 456,459 |
| ordered identity SHA-256 | `08c477a2f71455a72778f076f9942789a9997ab3b3008fc2a3d043f02458f062` |
| global original-input bitmap SHA-256 | `0d7794467640c727b89ccd5f170fc4430228e0aba6fa9d7f8657b0e653ef3912` |
| global bitmap 크기 | 529,200 bytes |

각 실행의 source SHA, 실제 실행 파일 hash, launch 환경, 출력 JSON과 manager receipt를
Rust 비교기로 묶어 검증했다. 일반 실행 중 source와 바이너리를 고정했고 별도 빌드나
agent의 파일 작업을 중지했다. 모든 성공 관측은 exit 0, normal, no timeout,
자동 재시도 없음, process tree drain 및 잔여 descendant 0을 확인했다.
관측된 pressure event는 없었다. receipt의 null OOM delta를 숫자 0으로 해석하지 않는다.

이 검산은 최종 해와 전체 원본 입력 union의 일치를 확인한다. P7P4의 모든
solution×source 관계를 개별 행으로 전부 증명한 결과는 아니다. 공급·Hold·중복·EOF의
세부 의미는 아래 작은 독립 oracle 회귀로 보완했다.

### 일반 성능 실행

메모리는 모두 manager의 `linux-sampled-process-group-rss`이다. kernel HWM,
제품 내부 RSS sampler 및 논리 owner 용량과 구분한다. 진단 실행은 이 평균에 포함하지 않는다.

| 조건 | 실행 | 탐색 시간(초) | 평균(초) | 표본 RSS 범위(MiB) |
|---|---|---|---:|---:|
| 이전 Trace, `5c14a657` | T11C/T11D | 148.340766 / 147.902314 | 148.121540 | 4,180.457–4,413.500 |
| 최종 Trace, `853305ad` | R1A/R1B | 32.384672 / 34.667556 | 33.526114 | 3,249.699–3,289.184 |
| 최종 Summary, Boolean ON・shared OFF | Q0A/Q0B | 39.944962 / 35.481240 | 37.713101 | 3,304.734–3,306.715 |
| 최종 Summary, Boolean ON・shared ON | Q1A/Q1B | 23.475621 / 23.787404 | 23.631513 | 2,605.676–2,792.277 |
| 최종 Summary, Boolean OFF・shared OFF | B0A | 39.741073 | 단회 | 3,489.938 |

Trace는 평균 시간이 77.366%, 평균 표본 메모리가 23.913% 감소했다. 이는
동일 입출력 계약을 가진 **여러 개선을 합친 구현 간 관측 비교**다. 어느 한 변경의
단독 효과라고 해석하지 않는다.

Q0A→Q1A→Q1B→Q0B 순서는 동일 바이너리에서 shared saturation만 바꾼 ABBA다.
평균 시간은 37.339%, 평균 표본 메모리는 18.354% 감소했다. 각 조건 두 번의
관측이며 실행 순서나 환경 변동이 통계적으로 제거됐다고 주장하지 않는다.
Q1A/Q1B 모두 exact local Ω 게시 1회, 다른 owner의 승인 8회, 실제 partial owner
저장 공간 해제 8회를 기록했다. Trace R1A/R1B는 설정이 ON이어도 이 세 카운터가
모두 0이었다. 출력 의미에 맞는 적용 경계가 실제 실행에서도 유지됐다.

B0A는 Boolean 전환 단독 효과를 살펴본 단회 관측이다. Q0 평균과의 차이는 작고
샘플도 부족하므로 독립적인 확정 speedup으로 채택하지 않는다. 큰 추가 이득은
여러 worker가 shared Ω 이후 full product 계산을 함께 중단할 수 있는 구조에서 관측했다.

초기 복원본 `7d04c8c9`의 physical-prefix ABBA는 OFF 38.46/50.18초,
ON 39.27/49.73초였다. 평균 차이는 ON +0.404%로 방향성 있는 개선을 확인하지
못했다. 동일 arm에서 실제 product 작업량은 약 4–5%, 시간은 약 26–30% 달라졌다.
관측하지 않은 CPU 주파수·throttling·외부 부하를 원인으로 단정하지 않는다.
최초 38초 실행만 골라 보고하지 않고 이 변동도 원시 자료에 보존했다.

## 4. 복원한 계산 생략과 정확성 경계

### 4.1 후보별 의존성 product와 ILC 검사 위치

일반 PC의 자격을 만족하는 요청은 완성된 기하 후보마다 배치 관계를 만들고,
그 물리적으로 가능한 미노 순서 언어를 기존 공급 product와 교차한다. 이전
`5c14a657`의 일반 BuildUp 경로 전체를 같은 방식으로 반복하는 비용을 줄였다.

자동 의존성 경로가 기존 완료 후보 ILC 검사 전에 반환하는 접점을 발견했다.
검사를 물리 관계·언어 생성 직전으로 복원했다. 필요한 삭제 행과 contributor를
낙관적으로 허용해도 closure가 멈출 때만 거절하므로, 통과를 성공 증명으로 쓰지 않는다.
physical-prefix를 끈 S0A에서는 원시 후보 29,856,840개 중 24,910,200개가 이
완료 후보 검사로 제외되어 첨부 기록에 나온 구조를 실제로 확인했다.

physical-prefix ON은 ILC와 first-clear 필요조건을 앞당긴다. 최종 SD2에서는
28,161,783개 후보를 불가능하다고 표시했고, 1,695,057개 생존 후보는 같은
immutable catalog/rows의 private ILC 증명을 재사용했다. raw 후보 전체는 그대로
내보내므로 원시 순서·개수·digest가 유지된다. digest가 필요 없는 허용된
CoverageSummary는 exact continuation count로 원래 ordinal을 유지하면서
subtree 자체를 생략할 수 있다. 동일 계산을 이름만 바꾸어 두 번 수행하지 않는다.

### 4.2 fresh 7-bag의 공급 조건과 구성적 존재 증명

초기 empty Hold가 허용된 fresh 7-bag은 첫 6 placements가 서로 달라야 하고,
Hold가 금지되면 첫 7 placements가 서로 달라야 한다. 이 필요조건을 물리 frame
생성 전에 적용한다. fixed/finite 공급, 이미 진행된 bag, 비어 있지 않은 초기 Hold,
부분 관측 등의 요청에 같은 조건을 무조건 적용하지 않는다.

Boolean 소비에는 더 강한 구성 증명을 사용한다. 충분한 실제 draw를 가진 원본
fresh 전체 7-bag, 종류별 최대 두 번, Hold 허용 시 7–11 placements와 실제
draw 수 ≥ placements+1, Hold 금지 시 8–11 placements와 draw 수 ≥ placements,
FullQueueOracle 등 모든 조건이
맞으면 가능한 실제 공급을 구성할 수 있다. Hold의 경우 첫 여섯 종류를 A…F,
빠진 종류를 G라 두고 첫 bag을 G,A,…,F로 만들어 G를 Hold에 둔다. 짧은 suffix에서
G가 두 번 필요하면 첫 G를 Hold에서 사용하면서 suffix와 충돌하지 않는 종류를
보관하고, 두 번째 G를 다음 bag에서 꺼낸다. 이 구성은 존재 여부만 증명한다.
최소 원본 ordinal이나 CountAll multiplicity를 대신 계산하지 않는다.
잘리지 않은 전체 원본 공급과 projection=false를 확인하고, 같은
`SourcePrefixConstraint`를 실제 적용해 생성한 nonempty 언어에만 이 증명을 소비한다.

초기 구현은 FullQueueOracle identity 전체를 ID 0인 편의 생성자와 비교해 실제
compiler가 넣은 요청 provenance ID 때문에 발동하지 않았다. `e1033027`에서
정책 의미만 검사하되 실제 provenance·projection·source 제약은 유지하도록 수정했다.
실제 공개 opening compiler를 거치는 회귀를 추가했다.

최종 SD2의 Boolean membership 353,240회는 전부 이 source certificate로
판정됐고, full global merge는 103,219회였다. 이 둘을 합하면 최종 identity 수다.
Q1A/Q1B에서도 Boolean 호출은 각각 341,929/357,095회이고 모두 certified였다.
이 P7P4 실행에서는 일반 Boolean DFS나 음성 cache가 효과의 원인이라고 주장하지 않는다.

일반 Boolean 탐색이 필요한 경우에는 첫 성공에 종료하며, 초기 root를 끝까지
탐색한 확정 음성만 저장한다. 취소나 부분 탐색을 음성으로 저장하지 않는다.
언어 arena의 소유권 토큰을 cache identity에 포함하여 숫자 root ID의 재사용을 막는다.

### 4.3 공유 Ω 이후의 full coverage 저장 공간 해제

Summary/Omit/CountUnique에서 전체 원본 입력 집합 Ω의 커버리지가 이미 확정되고,
개별 행·capture·typed coverage·추가 실행 제약 등을 요구하지 않으면 이후 후보에는
성공하는 원본 입력이 하나라도 있는지만 필요하다. 지금까지는 worker마다 자기
local coverage가 Ω가 될 때까지 큰 자료를 유지했으므로, 다른 worker가 요청 전체를
완성했어도 full product와 union 작업을 계속하는 문제가 있었다.

새 `SharedCoverage`는 생성 시 실제 request Arc에 결속된다. exact completion
증명의 수신자는 같은 request Arc, 실제 immutable tables Arc와 원본 입력 개수를
확인한다. 원본 입력 개수가 같다는 이유만으로 다른 요청의 증명을 쓰지 않는다.

현재 후보의 root/capture 소비, local merge와 `aggregate.observe`가 성공한 뒤
처음으로 local Ω를 게시한다. 다른 worker는 다음 후보 경계에서 증명을 받고
pending root를 flush한 다음 full DD·product memo·union 저장 공간을 해제한다.
자기 partial bitmap과 count는 사실대로 보존한다. shared completion은 기존
`global_is_complete()`나 최소 ordinal witness 권한을 확대하지 않는다.

완료 bitmap은 66,150개의 word를 갖는다. shared Ω 이후에도 후보마다 이를
다시 순회하는 병목을 만들지 않도록 입력 수를 먼저 확인하고 exact complete
Acquire latch로 O(1) 반환한다. 부분 coverage에는 기존 비교를 유지한다.
owner별 1회 사건만 계측하며, 후보별 새 cache나 반복 span을 추가하지 않았다.

### 4.4 ordered kick·공급 전이·EOF·메모리 회계

역방향 이동 관계에서 앞선 kick의 footprint가 이미 free인 source/target 안에
포함되는 등 first-success가 전역적으로 증명된 경우에만 불가능한 뒤쪽 kick을
생략한다. forward의 kick 순서와 원래 kick index는 유지했다. 5개 규칙과 7개 미노,
독립 scoring 기준의 회귀로 확인했다.

Current에서 얻은 동일 공급 draw를 Swap에서도 재사용하고, StoreCurrent에만
두 번째 advance를 계산한다. 원래 입력 ordinal·가중치와 서로 다른 Hold history를
합치지 않는다. 종료 Hold 방출도 각 입력의 **실제 이전 draw와 현재 EOF**로
판정하도록 보정했다. 길이가 다른 `[O]`, `[O,T]`, 초기 Hold T에서 O,T를 놓는
CountAll 결과 3과, P7의 유한 공급 길이를 3으로 제한했을 때 원본 24개 suffix
ordinal을 보존하는 동작을 별도 검증했다. 후자는 VisibleSeven 관측 정책 시험이 아니다.

memo 용량 합산은 증분 값으로 유지하고 실제 `clear_and_release`는 backing
allocation까지 해제한다. 이미 넘긴 geometry family의 과거 dispatch 용량과 현재
retained bytes를 분리했다. 논리 peak 추계에서 과거 용량을 숨겨 수치만 낮추지 않았다.

## 5. 구간 시간과 메모리 수명

진단 D11/SD1/SD2/RD1은 일반 시간 평균에서 제외했다. 다음 phase는 caller의
실제 한 번의 구간 시간이다. `work_and_wait`에는 caller의 실제 작업도 포함된다.

| 진단 / 정책 | plan(초) | worker 작업·대기(초) | merge(초) | canonicalization(초) |
|---|---:|---:|---:|---:|
| D11 이전 Trace | 0.759748 | 153.333210 | 0.152428 | 0.883150 |
| RD1 최종 Trace | 1.040916 | 32.284670 | 0.067257 | 0.950613 |
| SD1 공유 완료 개선 전 Summary | 0.746151 | 35.894089 | 0.131903 | 0.786299 |
| SD2 최종 Summary | 0.730147 | 20.074097 | 0.075880 | 1.049840 |

계획·병합으로 큰 병목이 옮겨간 관측은 없다. 최종 정렬은 SD2에서 약 1.05초로
증가했으나 전체 작업 구간 감소를 상쇄하지 않는다. 후보별 의존성·이동 검증이
남은 주요 비용이다. SD2의 같은 stage별 확장 누적치는 placement dependency
75.22초, 그 안의 coverage product 18.28초, upper 20.24초, reverse 21.78초다.
이는 worker와 caller의 중첩된 표본 확장 작업량으로 wall time에 더할 수 없다.

RD1 geometry의 확장 추정치는 52.38초로 D11의 22.11초보다 크다. 반면 실제
표본 누적은 0.249151초와 0.243645초이고 표본 수는 모두 323,748이다.
이를 숨기거나 geometry의 실제 wall 비용이 그 배수로 증가했다고 단정하지 않는다.
초기 physical-prefix ABBA도 단독 이득을 확정하지 못했으므로, 모든 하위 구간이
빠르거나 새 병목이 전혀 없다는 주장은 하지 않는다.

### 최종 Summary의 phase별 RSS

다음은 **제품 내부 sampler** 값이다. 앞의 manager 표본 RSS와 다른 측정이다.

| SD2 구간 | 관측 Peak RSS(MiB) | 구간 마지막 RSS(MiB) |
|---|---:|---:|
| pre-dispatch | 3.105 | 3.105 |
| geometry preparation | 15.195 | 15.195 |
| parallel verification | 3,028.992 | 731.027 |
| merge | 721.801 | 700.270 |
| result finalization | 764.082 | 683.594 |

병렬 peak는 시작 후 11,219ms에 관측됐다. 그때 가장 오래된 worker publication은
914ms 전 값이었다. 마지막으로 게시된 StandardBag 용량은 2,398.653MiB,
piece language는 88.107MiB, 결과 identity는 19MiB였다. product memo
122.083MiB와 union memo 1,176MiB는 StandardBag의 **부분집합**이므로 더하지 않는다.
이 publication은 정확히 동시적인 heap census가 아니며 RSS를 완전히 설명하지 않는다.

### worker 종료 시 보유한 실제 자료

다음은 각 worker 종료 checkpoint의 용량 합이며 동시 peak가 아니다. 서로 다른
수명과 포함 관계를 가진 행을 합쳐 RSS로 해석하지 않는다.

| 자료(bytes) | SD1 공유 개선 전 Summary | SD2 최종 Summary | RD1 최종 Trace |
|---|---:|---:|---:|
| StandardBag 전체 | 1,625,897,656 | 4,762,800 | 2,384,246,112 |
| product memo payload — 위의 일부 | 113,249,800 | 0 | 168,487,088 |
| union memo payload — 위의 일부 | 704,643,072 | 0 | 1,056,964,608 |
| piece language | 274,131,072 | 444,972,548 | 281,023,996 |
| worker scratch 전체 | 1,900,103,968 | 449,810,588 | 2,665,345,348 |
| 반환 identity | 99,614,720 | 99,614,720 | 109,576,192 |

SD2 StandardBag에 남은 4,762,800 bytes는 worker 9개의 529,200-byte bitmap이다.
full memo는 해제됐지만 실제 local coverage는 유지했다. 요청 수준의 불변 자료는
공유하고 별도 복제하지 않는다. 같은 request/table 결속, 마지막 Arc의 해제,
취소와 언어 arena recycle을 회귀로 확인했다.

piece language 종료 용량은 SD1보다 SD2에서 커졌다. bag+language가 같은
recycle 기준을 사용하므로 bag 해제는 language 재활용 시점에도 영향을 줄 수 있다.
이 자료가 사라졌다고 표현하지 않는다. 현재 결과에서는 scratch 전체가 크게
줄었지만 language의 phase별 수명은 후속 최적화 후보다. 원인을 입증하지 않고
마지막 RSS 차이를 leak이나 allocator retention으로 단정하지 않는다.

Trace는 최소 원본 ordinal과 대표 경로의 의미가 필요해 Summary 공유 증명을
그대로 적용하지 않는다. 종료 시 full 자료가 더 남는 차이를 정확성 경계와 함께
기록한다. 낮아진 RSS가 OOM 방지를 보장하는 것은 아니다.

## 6. GUI·다른 제품 옵션과 검증 범위

이전 `49fd03da`에서 완료한 GUI·Setup·B2B·Finesse·진행 이벤트·Node GC 개선은
유지했다. 자세한 동작과 당시 실행은 [GUI 성능 검증](gui-option-performance-2026-11-24.md),
[옵션 연결 표](gui-option-coverage-2026-11-24.md),
[App 재생 수명](app-replay-lifetimes-2026-11-24.md)을 참조한다.

| 요청 또는 옵션 | 최종 경로의 의미 |
|---|---|
| 일반 고정 필드 CountUnique | 자격이 맞으면 후보별 의존성 product 자동 선택 |
| Summary/Omit | exact Ω 이후 Boolean 존재성, native 병렬에서는 request-bound shared 완료 이용 |
| Trace·대표 경로·원본 최소 ordinal | 해당 증거를 계산하는 경로 유지; 공유 Summary 권한을 적용하지 않음 |
| CountAll | Hold 동작 multiplicity를 보존하는 concrete 경로 및 의미 가지치기 유지 |
| B2B·Score·Finesse | 실제 clear·spin·ordered kick 증거가 필요한 경로 유지; 확정 거절과 suffix 불가능성만 조기 제외 |
| 규칙·180·ordered kick | 정규화된 실제 규칙으로 관계 생성; 순서와 kick witness 보존 |
| VisibleSeven·typed probability·capture·개별 coverage | 각 consumer의 관측·coverage 계약 유지; fresh 전체 공급 존재 증명을 무조건 사용하지 않음 |
| fixed/finite/mixed-length/초기 Hold | 원본 공급 product, 실제 draw와 EOF 계약 유지 |
| Build·Setup·Recovery | 기존 semantic pruning과 product별 소비 계약 유지; Summary 전용 경로 강제 적용 없음 |

베타 사전 분석 UI 제거, Recovery의 5→4→3→2→1 반복 감소, 사용하지 않는 GUI
GPU 진입점 정리, CPU Auto와 실제 Setup/Recovery auto 유지는 앞선 변경을 보존한다.
Node GC ACK는 해당 main isolate의 정확한 PID/request에만 결속하며 Rust 메모리
회수를 증명하지 않는다. 이번 Rust 메모리 개선은 owner와 allocation의 수명 변경이다.

최종 source 검증은 다음과 같다. 괄호의 로그는 증거 묶음에 포함한다.

| 검증 | 결과와 범위 |
|---|---|
| 집중 Core 회귀 | 129 passed, 0 failed; 큰 P7 matrix 1개는 여기서는 ignored 후 별도 실행 (`shared-saturation-focused-tests.log`) |
| P7 matrix | 하나의 managed test 안 25 case 통과: 규칙 5×Hold 2×CountUnique/CountAll 2의 20개와 SrsPlus/Hold/CountAll spin 5개 (`shared-saturation-p7-options.log`) |
| benchmark 입력·출력 계약 | 31 passed (`shared-saturation-benchmark-contract-tests.log`) |
| native 제품 구성 | `clearra-app --no-default-features --features parallel` check 성공, 경고 없음 |
| WASM 제품 구성 | `clearra-wasm --target wasm32-unknown-unknown` check 성공, 경고 없음 |
| 공개 App product 실행 | `7d04c8c9`에서 작은 보드 5×spin 2의 10 case 통과; 독립 full scoring+late B2B와 paged witness·ordinal·weight 비교, cancel/budget/마지막 Arc 확인 |
| 새 공유 수명 회귀 | 위 Core에 bitmap 2, owner 5, bridge 3 포함. owner oracle은 240개 설정·언어 조합의 cold/warm/recycle 비교 |
| 최종 P7P4 | 모든 신규 일반·진단 실행의 완주·원시 후보·ordered identity·global bitmap 검산 통과 |

P7의 NoKick+Hold에서 coverage 5,024는 기준과 일치하는 결과다. 모든 경우를
무조건 5,040으로 맞추지 않았다. P7의 distinct 공급만으로 잡히지 않는 EOF·중복·
원본 suffix 수·Hold history는 별도 작은 oracle 회귀로 확인했다.

공개 App 10 case는 `853305ad`에서 다시 실행했다고 표현하지 않는다. 최종 변경의
native bridge/owner 회귀와 제품 compile check를 추가했다. GUI 54개와 Build/Recovery
26개도 이전 `49fd03da` 검증 기록이며 최종 source에서 재실행한 새 결과가 아니다.
전체 workspace test, 모든 GUI 조합, 모든 thread interleaving, allocator 실패 주입을
검사한 것은 아니다.

## 7. 재현 자료와 남은 경계

원시 보고서·영수증·launch·검산 도구·관련 리뷰·실제 측정 바이너리를
`Clearra-P7P4-restoration-evidence-2026-11-24.tar.gz`에 보존한다. 이 파일의 Linux
바이너리는 실험 재현 증거이며 배포물이나 설치 프로그램이 아니다. repository 전체나
credential 파일은 포함하지 않는다. manifest의 SHA-256으로 각 자료를 식별한다.

주요 결과는 `restoration-final-all-validated.v2.json` 및
`restoration-final-all-analysis.v2.*`이다. 비교기는 filtered 후보와 full raw 후보,
Trace와 Summary, 일반 실행과 진단, 누락된 역사 필드를 구분한다. 예전 v1 비교는
최종 판정에 사용하지 않는다. portable record는 원문 JSON을 바꾸지 않고 경로만
보존된 바이너리·보고서에 연결한다.

후속 개선의 우선 후보는 exact Ω 이전의 full coverage peak, 공유 완료 뒤의
piece-language recycle, Trace의 최소 ordinal 계산이다. 이번 결과만으로 더 큰
입력의 완주 가능성을 추정하지 않는다. Full 6L 재개와 CI·배포·main 승격은
사용자의 보류 지시를 유지한다.

# Build 생략, 일반 전이 재사용, Cloud Run 응답 경계

## 1. 작업 범위와 복구 근거

사용자가 제공한 100% 확률은 확정 전제다. 이 작업은 그 확률을 검색하거나
추가 탐색·증명하지 않는다. Full 6L 전체 열거, 작은 입력 성능 벤치마크, CI,
배포도 실행하지 않는다. 검증은 유한한 정확성 fixture와 로컬 빌드로 한정한다.
커밋과 별도 작업 브랜치 push는 사용자가 승인했다.

이전 scratch 작업공간의 미게시 소스 6개 커밋은 유지보수 후 사라졌다. 해당
브랜치와 commit object를 GitHub에서 찾지 못했으며, 이전 patch도 복구하지
못했다. 따라서 이번 구현을 이전 SHA의 복원본이라고 표현하지 않는다.
공개된 기준 `27dfd11440eb144f8c22329cdd56e2727034a450`에서 설계와 남은
기록을 바탕으로 재구현하고 새 소스로 검증했다.

반면 scratch 밖에 남은 manager receipt의 `stdout_tail`에서는 두 원본 진단
JSON을 복구했다. 추출에는 Rust를 사용했다. 다음 기록은 **과거 실행 근거**이며
새 구현의 실행 시간이나 메모리 실측으로 사용하지 않는다.

| 항목 | 첫 완료 후보 창 | 첫 줄 삭제 가지치기 이후 창 |
| --- | ---: | ---: |
| receipt | `1795402892422711228-3-runtime` | `1795404734223483158-3-runtime` |
| 당시 source | `cc295a0eb0d1af56463e5394720dfef17b83bfaa` | `6314e1eb643252d6a392fc1e27eb6c5d17ed0a24` |
| 완료 verifier 호출 | 100,004 | 100,001 |
| 실제 source에서 허용된 후보 | 3,833 | 16,757 |
| 허용 순서를 얻지 못한 후보 | 96,171 | 83,244 |
| graph node 합계 | 2,938,049 | 7,908,075 |
| reachability state 합계 | 41,745,284 | 119,433,218 |
| 준비 시간(초) | 143.636619451 | 106.550111160 |
| 준비 이후 joined return까지(초) | 2.381789373 | 3.698451462 |
| 관측 RSS peak(bytes) | 2,201,100,288 | 2,552,541,184 |
| process VmHWM(bytes) | 2,200,383,488 | 2,888,458,240 |

두 창의 후보 집합과 소스가 다르다. 위 시간으로 A/B 가속률이나 전체 소요 시간을
계산하지 않는다. raw family의 247,794,216,021,372 paths는 물리 해법 수가 아니다.
두 번째 창의 음성 verdict 역시 물리 불가능과 source/Hold/EOF 불일치를 구분하지
않으므로, 그 verdict만으로 새로운 물리 가지치기를 만들지 않는다.

원본 JSON은 `_local/artifacts/build-elision-cloud-boundary-20261123/recovered-receipts/`
아래에 보존했다. 첫 창의 32개 대표 후보는 piece와 target-frame mask만 정확히
추출해 `geometry_first_clear_feedback_fixture.rs`의 유한 회귀 입력으로 남겼다.
추가 witness 탐색이나 BuildUp은 수행하지 않는다.

## 2. Cloud Run 실패 응답과 실제 연산 종료

Discord 계산 계열의 기본 제한은 모두 0이다. PC, Build, Setup, forward,
structure, utility, diagnostic, direct executor 및 대기열에서 0은 타이머가
없음을 뜻한다. 호출자가 명시한 양수 제한은 계속 지원한다.

Cloud Run의 HTTP 요청 예산은 계산 제한과 별도다. 현재 배포 설정 계약은
900초이며, 응답을 보낼 여유 15초를 남긴다. Oracle은 POST 직전에 제출 시각을
기록하고 서버는 body 읽기 전에 ingress 시각을 확보한다.

\[
D = \min(t_{\mathrm{POST}},t_{\mathrm{ingress}}) + 900\,000 - 15\,000
\]

이 경계에는 네트워크·cold start·요청 안의 대기·body 읽기가 포함된다.
POST 이전 Oracle 자체 대기열에는 아직 Cloud HTTP 요청이 없으므로 이 예산을
부과하지 않는다. 미래의 client timestamp로 서버 예산을 늘릴 수 없고,
같은 job ID를 다시 제출해도 기존 작업의 deadline이 새로 시작되지 않는다.

경계에 도달하면 `E_CLEARRA_PLATFORM_REQUEST_DEADLINE` 실패를 먼저 확정하고
runner에 취소를 전달한다. HTTP 응답 완료와 child 정리 완료는 다른 상태다.
실제 child close와 artifact cleanup이 끝나기 전에는 실행 슬롯을 반환하지 않는다.
뒤늦게 도착한 성공·실패·취소는 확정한 terminal 결과를 바꿀 수 없다.
클라이언트도 fetch의 abort 준수만 믿지 않고 실패 promise를 별도로 확정한다.

이 분리가 필요한 근거는 Cloud Run의 [request timeout 문서](https://docs.cloud.google.com/run/docs/configuring/request-timeout)와
[container contract](https://docs.cloud.google.com/run/docs/container-contract)다.
504로 연결이 닫혀도 container가 종료된다고 보장되지 않으며, startup 시간도
request timeout에 포함된다. 운영 service를 변경한 것은 아니며, 이번 변경은
배포 전 소스와 로컬 계약 검증이다.

14분의 Discord 전달 설정은 공개 검색의 결과 전달 경로만 바꾼다. 승인된 공개
검색은 interaction 전달 창 이후 원래 채널로 완료 또는 실패를 보낼 수 있다.
관리·비공개·접근 거절·입력 검증 오류에는 공개 채널 fallback 권한이 생기지 않는다.
다른 메시지로 옮기는 첨부는 원래 bytes에서 다시 업로드한다.

Undici 6.28.0을 고정하고, 동기 job의 응답 header 대기 제한만 0으로 지정했다.
기존 proxy/pool과 body 정체 제한, 출력·첨부 크기, 취소 요청 제한은 독립적으로
유지한다. Oracle 배포 레이어와 relocated import 검증에도 새 runtime 의존성을
반영했다. Cloud readback은 service/revision의 실제 900초 설정과 계산0,
900000ms 요청 예산, 15000ms 응답 여유를 함께 검증한다.

## 3. 부모 배치 집합에서 줄 삭제를 한 번 계산

원본 row r의 기여 operation 집합을 C_r, 현재 배치된 집합을 A, 다음 operation을
i라 하면 새 줄 삭제 조건은 다음과 같다.

\[
C_r \setminus A = \{i\}
\]

이 식을 부모 상태마다 모든 완료 대상 row에 한 번 적용해 `[u8; 15]`에 다음
operation별 clear 개수를 기록한다. 각 자식의 모든 pose·kick은 같은 값을 읽는다.
Catalog가 원본 footprint와 실제 ILC pose의 동일성을 이미 확인한 경로에서는
자식 보드의 재배치·압축·비교를 반복할 필요가 없다.

부모에 자식 후보가 d개, 높이가 H이면 이 부분은 반복적인 O(dH) row 계산에서
O(H+d)로 바뀐다. Stack 배열 15 bytes를 사용하고, 후보별 heap cache를 더하지
않는다. 실제로 방문하지 않는 자식의 projection도 먼저 만들지 않는다.
Catalog 권한이 없는 임의 pose helper는 기존 구체 검증을 유지한다.
이 전이 재사용은 큐 종류·piece 이름·킥 profile을 조건으로 삼지 않는다.

## 4. BuildOrder와 물리 subset projection을 생략하는 제품 경로

`placement_poset::compile_rigid_columns`는 원본 프레임 하나에서 충분조건을 검사한다.
모든 선택 skeleton이 frame 0의 실제 tetromino여야 한다. 그 다음 아래 두 방향을
별도로 확인한다.

1. **필요관계.** 바닥이나 초기 보드에 직접 지지되지 않는 operation에서 가능한
   원본 바로 아래 support owner가 하나이면 그 owner는 필수 predecessor다.
   해당 support row가 먼저 지워져도 그 row를 채우려면 owner가 이미 배치되어야 한다.
   여러 owner의 OR를 임의의 하나나 AND로 바꾸지 않고 Unknown으로 반환한다.
2. **충분성.** transitive predecessor만 포함한 최소 보드에는 footprint 접점까지
   바닥에서 끊기지 않는 column이 있어야 한다. 자신과 successor를 제외한 최대
   보드는 전체 수직 진입 corridor를 비워야 한다.

미배치 rigid footprint는 연속된 원본 row를 점유한다. 이 row들은 해당 operation
배치 전에는 지워질 수 없으므로, 이후 프레임에서도 footprint는 수직으로만
이동한다. 완료 row를 삭제해도 바닥까지 연속인 지지 column에 틈이 생기지 않고,
원래 빈 수직 corridor에 blocker가 새로 들어오지 않는다. 따라서 poset의 모든
ideal 전이는 harddrop으로 가능하다. 모든 실제 순서가 필요관계를 만족하고,
모든 linear extension이 물리적으로 가능하므로 정확한 순서 언어가 된다.

인증 비용은 O(N²+NH)의 작은 bit 연산이고 stack 배열만 사용한다. 2^H 삭제
프레임이나 2^N 배치 부분집합을 먼저 순회하는 실패 prepass는 추가하지 않는다.
Gap skeleton을 발견하면 즉시 Unknown으로 돌아간다.

인증에 성공하면 기존 `compile_poset_language`와 일반 `cover_piece_language`를
연결한다. BuildOrderGraph, 물리 subset projection, movement BFS를 만들지 않는다.
같은 종류라도 predecessor와 successor가 다른 operation은 합치지 않는다.
양쪽 관계가 같은 동종 작업만 count 좌표로 압축한다. 언어 표현의 4096-state
admission에 실패하면 기존 verifier로 돌아가며, 후보나 검색 공간을 잘라내지 않는다.

제품 gate는 CountUnique·ClearToEmpty의 Boolean 물리/공급 계약에 한정한다.
요청한 trace, spin/finesse, score, 실행 제약, 관측 정책은 기존 생성 경로를 사용한다.
기본 Trace 출력에서도 실제 representative가 필요하지 않은 후속 후보에는 적용할
수 있다. 일반 fixed/pattern/symbolic source와 Hold·EOF는 기존 원본 교집합 owner가
판정한다. source 권한이 없으면 fallback이다. imported kick을 포함해 같은 표준
shape/all-sky 진입 계약을 사용하는 경로에 별도 profile-name 분기를 두지 않는다.

이 충분조건이 모든 후보의 Build를 제거하는 것은 아니다. 특히 원본에 gap이 있는
ILC skeleton은 admission 밖이다. 원본 대형 후보에서의 admission 결과와 전체
성능 효과는 구분하여 아래 검증 결과에 기록한다.

선택한 profiling 빌드에는 `wasm.candidate.build_order_certificate_checks`와
`wasm.candidate.build_order_certificate_elisions`를 추가했다. 각각 실제 인증 시도와
공급 교집합까지 완료한 Build 생략 수이며 sampled 시간 추정치와 합치지 않는다.
일반 제품 빌드에서는 이 계측 호출이 no-op으로 컴파일된다.

## 5. 일반 킥 전이와 첫 줄 삭제 이전의 가지치기

회전의 source S와 선택 target T가 이미 비어 있다면, 이전 kick footprint K_j의
충돌 조건은 `K_j & !(S | T)`로 축약할 수 있다. 이전 후보들이 모두 충돌해야
선택 kick이 first-success가 된다.

\[
\bigwedge_j (B\cap K'_j\ne\varnothing)
\]

교집합과 정확히 같은 **실제 clause**가 존재할 때만 이 conjunction을 단일
mask test로 바꾼다. Singleton이면 한 bit test가 된다. 단순히 교집합이
비어 있지 않다는 이유로 바꾸지는 않는다. 예를 들어 `{a,b}`와 `{a,c}`는 a가
비어 있어도 b와 c로 각각 막힐 수 있다. 일반 conjunction과 긴 imported prefix는
원래 순서를 유지한다.

전이당 기존 padding 1 byte에 축약 태그를 넣어 `ReverseRotationSource`는 계속
8 bytes다. Forward selector가 이미 확인한 target collision도 다시 확인하지 않는다.
Spin evidence의 predecessor 진입에는 같은 board의 downward occupancy shadow를
한 번 계산하고, 그 shadow 밖의 pose는 직접 수직 진입이 가능하다는 양성 증거로
재사용한다. 막힌 shadow는 Unknown이며 기존 역방향 탐색을 실행한다. Rank와
동점 선택 순서는 유지한다.

Geometry prefix에는 첫 줄 삭제 전의 원본 프레임을 사용한 불가능성 조건을 추가했다.
아직 선택되지 않은 target cells를 낙관적인 support로 주고, 선택된 frame-zero
operation을 가능한 supporter 관계로 closure한다. 이 낙관적 상태에서도 어느
완료 대상 row도 채울 수 없으면 전체 subtree를 버릴 수 있다. 미래 작업만으로
완성되는 row가 있거나 입력 권한이 불완전하면 Unknown이다.

이 조건은 큐·Hold와 독립적이다. 제품 physical-PC gate를 board64의 지원 차원과
출력 의미로 정의하여 원래의 Full6L/특정 source 조건을 없앴다. 외부 producer,
tiling-only 및 실행-family evidence의 원본 domain은 기존 계약을 따른다.
같은 selected prefix의 검증은 yield/resume과 Union/Product에서 재사용하고,
일반 Append와 병렬 seed Append에서 모두 무효화한다. 기존 task word의 여분 bit를
써서 native traversal record는 계속 72 bytes다.

## 6. 메모리와 실행 소유권

과거 rustc SIGKILL은 OOM을 우선 원인으로 취급한다. 당시 receipt에는 물리 RAM
10,451,423,232 bytes, 감독된 tree의 sampled peak 5,008,994,304 bytes,
마지막 가용 RAM 1,021,661,184 bytes가 남아 있다. 다른 프로세스의 사용량과
표본 사이의 peak도 있으므로 단일 child RSS가 RAM 총량보다 작다는 이유로
메모리 부족을 배제할 수 없다. Kernel OOM counter는 unavailable이어서
`kernel-confirmed OOM`으로 기록하지는 않는다. 자원 조건을 바꾼 OOM 재시도는 하지 않는다.

새 전이 태그는 기존 padding을 사용하고, parent clear와 충분조건 인증은 stack
작업이다. Build 생략 성공은 새로운 graph/projection 할당을 피한다. 이미 따뜻한
workspace가 가진 capacity를 매번 해제해 재할당 비용을 만들지는 않는다. 따라서
이 알고리즘의 공간 절약과 전체 프로세스 peak RSS 감소를 동일한 주장으로 쓰지 않는다.

Manager는 시간 제한 0에서도 owner stdin lease, 메모리 감독, 자손 정리,
출력 제한을 유지한다. Owner 종료 시 새 검색이 시작되지 않으며 자손을 정리한
뒤 collector를 join한다. WSL 검색의 0은 `RuntimeMaxSec=infinity`다.
기존 준비된 worker 즉시 투입, 전체 허용 CPU 사용, 단일 NUMA fallback,
Chromium의 yield 정책은 유지한다.

## 7. 검증과 남은 경계

Rust 1.98.1, Node 22.23.2, pnpm 11.5.0을 사용했다. 메모리가 큰 Rust 빌드와
검증은 한 번에 하나의 manager 작업으로 실행했다. 최초 새 빌드부터 test/dev
debug 정보만 껐고 worker 수나 메모리 한도를 줄이지 않았다. 현재 허용 논리 CPU는
9개다. 시간 수치는 정확성 테스트의 실행 기록일 뿐 작은 입력 성능 비교가 아니다.

| 검증 | 결과 |
| --- | --- |
| Manager Rust unit | 34/34 통과 |
| Node owner/deadline 및 검색 wrapper 계약 | 80/80 통과; detached owner 후속 3/3 통과 |
| 실제 짧은 child-process lifetime | 4/4 통과; 모든 tree drain 확인 |
| Discord·Cloud·HTTP·입력·의존성·배포 계약 | 262개 중 261 통과, 0 실패, Windows 전용 1 skip |
| Cloud readback/패키지 경계 후속 | 14개 중 13 통과, Windows 전용 1 skip |
| terminal/공개 전달/0 타이머·로그 최종 회귀 | 24/24 통과 |
| 코어의 범용 전이·줄 삭제·Build 생략·source 회귀 | 52/52 통과 |
| 최종 observer 포화와 제품 Build 생략 회귀 | 6/6 통과; 앞 행과 일부 중복 |
| 완료 후보 창·시간창 배제·준비 이후 profile delta | 3/3 통과; 실제 탐색 없음 |
| native `--no-default-features --features parallel` | `cargo check` 통과 |
| `wasm32-unknown-unknown` 제품 라이브러리 | `cargo check` 통과 |
| `clearra-cli-command` 라이브러리 | `cargo check` 통과 |
| 변경 파일 whitespace/format | `git diff --check`, Rust formatting 통과 |

동일 테스트의 후속 재검증이 포함되므로 행을 단순 합산하지 않는다. Source별
dead-code/unused 경고가 남아 있으며 경고가 전혀 없는 빌드라고 주장하지 않는다.
초기 두 compile 오류는 test record의 새 필드 누락이었고, no-default check에서는
승격된 언어 API의 `PieceKind` import cfg 누락을 찾아 수정했다. OOM 재시도와는
다른 소스 수정 후 컴파일이다.

제품 Build 생략 fixture는 실제 기본 `verify_candidate` 분기를 통과한다. fixed,
불일치 fixed, 초기 Hold+EOF, lookahead, 일반 pattern, symbolic bag의 6가지 공급
계약에서 기존 graph 결과와 buildability·count completeness·witness ordinal·coverage가
같았고, 새 workspace의 graph/projection 저장소는 비어 있었다. 별도 primitive
검증은 SRS, SRS+, SRS-X, Jstris 180, NoKick에서 도달 가능한 모든 fixture subset
및 piece-word 언어를 대조했다. 다른 successor를 가진 동종 operation은 합치지 않는
회귀와 삭제된 supporter·clear 후 떠 버리는 접점의 fallback도 통과했다.

첫 진단에서 복구한 32개 후보에 대해서는 다음을 확인했다.

- 첫 줄 삭제 조건은 기존 양성 16개를 모두 보존하고 음성 16개 중 15개를 제거했다.
  나머지 음성 하나는 Unknown으로 남긴다. 이 15/16을 전체 family 제거율로 외삽하지 않는다.
- 새 rigid-column 인증은 양성 16개와 음성 16개 모두 Unknown이었다. **이 표본에서
  Build 생략은 0/32**다. 따라서 이 인증만으로 Full6 후속 병목을 해결했다고 주장하지 않는다.
- 실제 harddrop witness의 선택 부분집합 32,768개도 첫 줄 삭제 조건을 모두 통과했다.
  선택 순서를 뒤집어도 유지되며, yield/resume·병렬 seed Append의 재검증도 확인했다.

최초 성공한 코어 test compile의 manager sampled peak는 2,279,911,424 bytes였다.
이는 새 **컴파일** 실행의 관측치이며 검색 RSS나 이전 OOM 원인의 재현값이 아니다.
이번 변경으로 대형 탐색의 peak RSS 또는 전체 실행 시간이 얼마나 감소했는지는
새로 실측하지 않았다. 과거의 서로 다른 후보 창을 그 수치로 대체하지 않는다.

완료 후보 진단은 `--completed-candidates N`을 명시했을 때만 실행된다. 완료된
verifier 결과만 집계하고 각 verdict의 첫 16개 sample을 보존한다. 요청의 cancellation
atomic 동일성으로 다른 동시 검색을 제외한다. 1초 RSS 관측 주기는 취소 deadline이
아니며, 후보 창을 넘어서 진행 중이던 완료 작업은 별도로 기록한다. 포화 시에는
`counts_exact=false`와 lower-bound 설명을 내보낸다. 일반 제품 빌드에는 observer가 없다.

검증 원본은 `_local/artifacts/build-elision-cloud-boundary-20261123/`의
`core-regression.log`, `core-final-contracts.log`, `core-no-default-fixed.log`,
`core-wasm-check.log`, `cli-command-check.log` 및 manager receipt에 있다.
Discord 계층은 `_local/artifacts/cloud-lifecycle-{final-tests,deployment-guard-tests,terminal-tests}.tap`에 있다.
추가 raw 출력은 연구 문서에 복사하지 않았다.

원격 작업 브랜치는 `codex/build-elision-cloud-boundary-20261123`이다. 일반 Git
쓰기 인증이 제공되지 않아 연결된 GitHub API로 동일 blob/tree를 게시했다. API의
작성자·시각 metadata 때문에 로컬 임시 commit SHA와 원격 SHA는 다르지만 게시마다
tree SHA가 완전히 같은지 재조회했다. 이전 로컬 checkpoint도 별도 branch로 보존했다.
Main 변경, PR 생성, force push, CI 조회·실행·대기, 실제 Discord 전송, Cloud 배포는 하지 않았다.

### 후속 작업

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/placement_poset.rs`, 구현 내용: gap-bearing ILC와 support OR를 보존하는 더 넓은 정확한 순서 표현을 검토하되, 비싼 실패 prepass 없이 admission할 수 있는 조건부터 찾는다.
- [ ] 수정 파일: `crates/clearra-core-executor/examples/full6l_bench.rs`, 구현 내용: 다음 승인된 대형 진단에서는 동일 candidate identity 집합과 동일 source에서 graph·reachability·coverage 비용 및 peak RSS를 대조한다. 전체 열거 시간으로 외삽하지 않는다.
- [ ] 수정 파일: `apps/clearra-discord-bot/CLOUD_RUN_JOB_SERVICE.md`, 구현 내용: 별도 배포 승인 후 실제 Cloud request 경계, terminal 실패 보존, child drain 및 Discord 최종 전달을 운영 환경에서 확인한다.
- [ ] 수정 파일: `.github/workflows/candidate-preflight.yml`, 구현 내용: 별도 CI 승인 후 확정된 원격 SHA에 필요한 검증만 수행한다. 이번 작업은 CI를 실행하거나 조회하지 않는다.

# 기존 탐색 경로 비용, 큰 필드 병목, 최적화 가능성

작성일: 2026-11-28 (Asia/Seoul).

## 1. 범위와 근거

사용자 결정에 따라 이전 row-stack 기하 생성기는 폐기된 실험이다. 이 문서는
그 생성기의 재실행·개선·제품 채택을 제안하지 않는다. 기존 exact-cover/의존성
경로, extended PC, forward damage/spin, spin-structure를 분석한다.

현재 실측 대상은 native host에서 실행한 Rust `WasmCpuSearchBackend`다.
이름에 WASM이 있어도 이번 자료는 브라우저 실행 시간이 아니며, C core나
tablebase를 사용하지 않았다. 초기 프로젝트 기획 문서의 CPU/GPU 구분을 현재
실행 경로의 증거로 사용하지 않았다.

확인한 소스는 `8a66d352a46ae1e2f46829b92303ab54a3db1d3f`와 후속 최소
진단 계측 `ff6fd359bb1791be5f5aa690412bbdcbb2881969`다. 이전 완료/취소 자료를 재사용하고 Rust로 수치와 구간 합을
재계산했다. 생성된 원본 자료는 `build/research/`에 보관한다.

- P7P4 기존 경로: `build/research/row-stack-20261128/reference-diagnostic-binary-profile.json`.
  소스 `a8f65517fe92252957ce5ba6656918aeb8bb057e`, binary SHA-256
  `31ce5c1e292b113f8fc612a432557a8771a9b687a83ce99384fa6cdd9ba1a2aa`.
  디렉터리 이름과 달리 해당 파일은 기존 경로(`row_stack_geometry_mode=0`)다.
- 고정 필드: `build/research/fixed-field-cost-20261128/h*.json`.
  소스 `8a66d352a46ae1e2f46829b92303ab54a3db1d3f`, binary SHA-256
  `196c7bc158405f3f81029f60d7ec0e2eb9f9e0f5485519895b77b1b8fb97b522`.
- 재계산: 같은 디렉터리의 `review_summary.rs`, `review-summary.json`.

두 계측군은 Linux, Rust 1.98.1, 논리 CPU 9개, 요청 워커 자동/전체 사용,
`parallel,local-search-ab,wasm-stage-profiling`, 유한 `benchmark-search` 감독을
사용했다. 서로 다른 입력·revision이므로 두 계측군의 시간을 직접 A/B 성능
비교로 사용하지 않는다. Discord 8 vCPU의 실행 시간으로 환산하지 않는다.

고정 필드 진단의 요청 스레드는 요청 접수·계산 owner 전달·대기·JSON 출력만
담당한다. 문제 컴파일·탐색·결과 해제는 별도의 계산 예산 owner가 수행한다.

## 2. P7P4: 전체 시간에서 실제로 큰 부분

입력은 빈 10열×4행, 10미노 배치, P7P4 11미노 공급, Jstris 180,
빈 Hold 허용, FullQueueOracle, CountUnique, Summary다. 기존 경로의 완료된
프로파일에서 backend wall time은 **14.255634106초**다.

| 구간 | 시간(ms) | 전체 backend wall 비율 |
| --- | ---: | ---: |
| 카탈로그 생성 | 1.357458 | 0.009522% |
| 공급 구조 생성 | 2.828016 | 0.019838% |
| 기하 owner 준비 | 3.159786 | 0.022165% |
| family 컴파일·병렬 분할 계획 | 725.960063 | 5.092443% |
| 병렬 초기화 | 0.041072 | 0.000288% |
| 병렬 탐색·검산 및 완료 대기 | 12,664.356830 | 88.837555% |
| 결과 병합 | 62.650147 | 0.439476% |
| 최종 coverage 처리 | 0.099560 | 0.000698% |
| 결과 생성·정규화 | 633.873201 | 4.446475% |
| 위 span 외 시간 | 161.307973 | 1.131538% |

이 표는 [parallel_search.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/parallel_search.rs)의
initialize → work/wait → merge 및
[result.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/result.rs)의
순차 span 수명을 대조해 만든다. `result_canonicalize`에는 key 생성, 결과
조립과 선택적 증거 준비도 포함되므로 이를 전부 정렬 비용이라고 부르면 안 된다.
증거 해시 생성 약 519.683ms는 backend 호출 밖이며 위 표의 분모에 포함하지 않는다.

이 입력에서 카탈로그·공급·기하 owner 준비를 전부 없애는 가상의 상한도
7.345260ms, 약 **0.0515%** 절약에 그친다. 개선 우선순위는 병렬 구간 내부의
반복 검산, family 준비, 결과 생성 순으로 잡는 것이 실측과 부합한다.

### worker 누적 시간은 전체 시간 비율이 아니다

[search_stage_profiler.rs](../../crates/clearra-core-executor/src/performance/search_stage_profiler.rs)는
첫 4,096 ordinal을 계측하고 이후 1,024개마다 표본을 얻어 확장한다.
`*_ns`는 표본 확장치, `*_measured_ns`는 관측 span 합이다. 서로 다른 worker의
구간은 동시에 실행되고 같은 worker 안에서도 부모·자식 span이 중첩된다.

P7P4의 worker 기하 순회 28.703초, 의존성 검산 47.352초는 이러한 누적
추정치다. 의존성 검산 안에 piece language 10.559초, 공급 language product
13.748초 등이 포함된다. 이를 모두 더하거나 14.256초로 나누어 배타적인
wall 비율로 표시하면 틀린다. 또한 worker elapsed는 OS CPU time 측정이 아니다.

### 기존 prefix 재사용은 큰 계산 절약이다

완료된 기존 경로는 raw 기하 후보 29,856,840개 중 28,161,783개,
즉 **94.3227%**를 공유 physical-prefix 증거로 생략했다. 살아남은
1,695,057개는 기존 ILC 증거를 재사용했다. 정규화 해는 456,459개다.
검증된 원 큐 4,233,600개의 coverage 결과를 재사용하며 이 문서에서는
그 확률을 다시 탐색하거나 증명하지 않았다.

같은 실행의 RSS 관측 최대는 3,049,029,632B(약 2.84GiB)다. 그 시점의
마지막 worker publication에서 standard-bag 논리 capacity 합은
2,655,710,696B, piece-language capacity는 99,550,964B, 공유 geometry
family는 6,030,752B였다. 서로 겹칠 수 있고 publication 시각도 달라 합산
RSS 귀속은 불가능하다. 다만 전체 메모리의 중요한 조사 대상은 생성기
작업 스택보다 공급/언어 cache와 결과 수명임을 보여 준다.

## 3. 지정 필드: 크기와 실행 경로를 함께 봐야 한다

각 fixture는 폭 10, 모든 행의 x=0,1에 초기 블록, 나머지를 채워 PC가 되는
목표, 단일 all-I 큐, Hold off, SRS+, CountUnique, Summary다.
I11·I15 fixture만 최상단 x=2..5에도 초기 블록을 둔다. 형상·미노 수·경로
자격이 함께 변하므로 다음 표는 하나의 변수에 대한 scaling 곡선이 아니다.

| 입력 | backend wall(ms) | 종료 상태 | 기하 후보 | 보고된 정규화 해 | kernel HWM(B) |
| --- | ---: | --- | ---: | ---: | ---: |
| 5행·I10 | 9.257930 | 완료 | 51 | 24 | 5,533,696 |
| 6행·I11 | 22.465303 | 완료 | 111 | 27 | 8,556,544 |
| 6행·I12 | 24.077933 | 완료 | 301 | 61 | 28,475,392 |
| 7행·I14 | 192.259908 | 완료 | 1,401 | 130 | 11,300,864 |
| 8행·I15 | 5,002.088138 | 5초 협력 취소 | 미완료 | 미완료 | 11,100,160 |
| 8행·I16 | 93.620550 | 동적 메모리 admission 거절 | 미완료 | 미완료 | 6,246,400 |
| 9행·I18 | 2,969.951093 | 동적 메모리 admission 거절 | 미완료 | 미완료 | 8,327,168 |

완료 4건의 수치는 backend가 보고한 결과다. 이 경계 진단은 독립 해법 parity를
검사하는 도구가 아니며, 취소·실패의 부분 결과를 정답으로 내보내지 않는다.

실제 소스 경계:

- `SearchProblem::pc_placement_dependency_eligible`: 추가 조건과 함께 최대
  11미노·compact 높이 6 이하를 요구한다. 11→12에서 경로 자격이 달라진다.
  I12 입력은 compact fixed-queue witness 경로를 사용했으며 언제나 전체
  물리 순서 그래프를 생성하는 것은 아니다.
- 높이 6→7: compact에서 extended PC로 바뀐다.
- `extended_buildup.rs`: operation 15개 이하는 `Vec<u32>`의 subset 인덱스,
  16개 이상은 `HashMap<u64,u32>`다. 이는 저장 표현 전환이며 515GB 요구를
  직접 설명하지 않는다. 16개에서도 처음에는 작은 map만 준비한다.
- required cells 64개 초과는 별도의 geometry 표현 경계다. I18 입력이
  여기에 해당한다.

### 확장 PC의 검산 비용

7행·I14 완료 실행은 1,401개 후보에서 reachability 상태 **13,804,902개**를
방문했다. worker 검산·축약 누적 788.500281ms 중 물리 순서·이동 판정은
778.912686ms, 약 **98.8%**다. 해당 후보 수에서는 span 전수 계측이다.
이 비율은 worker 검산 내부 비율이며 전체 192.260ms의 wall 비율이 아니다.

8행·I15 첫 5초에서도 worker 검산·축약의 표본 확장치는 37.744초,
물리 순서·이동 판정은 37.283초였다. 실제 표본 span 합은 각각
5.499928초와 5.453746초이고 두 단계의 표본 수는 37,144다. 기하 worker
span 합은 18.949ms였다. 전체 완료 시간을 추정하지 않아도 현재 관측
구간의 큰 비용이 **후보별 물리 검산**이라는 결론은 일관된다.

확장 경로에는 compact와 동등한 전체 wall 단계 분해가 아직 부족하다.
다음 계측은 family 완료/분할, dispatch/join, merge/result, subset 투영,
movement BFS를 나누어야 한다. 현재 누적치로 누락된 wall 비율을 만들지 않는다.

## 4. 수백 GB 메모리 요구: 실제 RSS와 다른 문제

기존 I16 결과의 `required_memory_bytes`는 **514,827,644,890B**이고 I18은
**46,328,584,048,175B**다. 이는 관측 RSS나 실제 할당 성공량이 아니다.
I16은 실패 전 `wasm.geometry.advance` 16,326회를 기록했다. 따라서
`execution_started=false`라는 ResourceReport 필드만 보고 "탐색 전 거절"로
설명할 수 없다. `ExecutionMemoryBound::ensure`가 실행 중 증설 거절에도
admission-failure report를 사용하고 있다.

[extended_pc_family_parallel.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/extended_pc_family_parallel.rs)의
`WorkerResult::reserve_branch`는 첫 후보를 검산하기 전에 branch 전체의
후보 수만큼 `Vec<u64>` digest 공간을 예약한다.

\[
M_{\text{digest branch}}=8C_{\text{branch}}+O(\text{segment 수}).
\]

기하 family가 거대한 후보 집합을 작은 구조에 압축해도, 이 보조 배열이
후보 수에 비례하는 공간을 먼저 요구한다. 원래 ordinal 순서대로 FNV 계열
digest를 결합하기 위해 실패한 물리 후보의 digest도 보존한다.
이 때문에 큐가 단 하나여도 큰 예약이 가능하다.

이 지점의 직접 확인을 위해 local-search-ab/native 빌드에만 선택적인
실패 JSON 계측을 추가했다. `CLEARRA_EXTENDED_PC_ADMISSION_DIAGNOSTIC=1`
일 때 실패한 branch ordinal/count, digest payload, 전체 private claim을
stderr에 남긴다. 예약량·오류 판정·계산 경로는 바꾸지 않는다.

### 짧은 실패 재현으로 발생 지점 확정

진단 소스 `ff6fd359bb1791be5f5aa690412bbdcbb2881969`를 release 빌드한
binary SHA-256은 `30fd21cba779662c01b8b0ddff62d0c8cf2af8d54d6ba8ffdf727f448c0ca196`다.
동일 9-worker 조건, 각 30초 supervisor 상한, 5초 협력 취소 조건으로
두 실패 입력만 순차 재실행했다. 둘 다 취소 전에 같은 거절을 재현했다.

| 입력 | 거절된 한 branch의 후보 수 | digest payload(B) | 전체 요구량(B) | kernel HWM(B) |
| --- | ---: | ---: | ---: | ---: |
| 8행·I16 | 64,353,280,429 | 514,826,243,432 | 514,827,644,890 | 6,402,048 |
| 9행·I18 | 5,791,072,718,810 | 46,328,581,750,480 | 46,328,584,048,175 | 8,015,872 |

두 JSON 모두 `stage=branch-digest-preallocation`에서 발생했다.
I16의 전체 요구량에서 digest payload를 뺀 값은 1,401,458B,
I18은 2,297,695B다. 즉 거대한 요구량은 **검산 전에 선예약하는 digest
배열로 직접 설명된다**. 동적 발생 지점까지 확인했으므로 추정 원인으로
남겨 둘 필요가 없다. I18에서 여러 worker가 같은 크기의 branch 거절을
기록했지만 이를 합쳐 하나의 성공한 할당량처럼 계산하지 않는다.

실패까지 backend wall은 각각 99.207236ms, 3,074.371676ms였다. 이는 원인
재현 시간이며 이전 93.621ms/2,969.951ms와의 속도 비교가 아니다.
`owner_is_requester=false`, `available_parallelism=9`를 다시 확인했다.
프로세스는 예상된 ResourceAdmission으로 종료했으며 감독 receipt의 종료 시
descendant 수는 0이다.

원본: `h8-i16-admission-origin.json/.stderr`,
`h9-i18-admission-origin.json/.stderr` (위 고정 필드 진단 디렉터리).
runtime receipt ID는 각각 `1795866180233264573-5`, `1795866201267414478-5`다.
기존 완료 실행과 독립 해법 parity를 새로 검사한 것은 아니다.

### 원시 기하 후보 수가 커지는 조합적 이유

I16 fixture의 8열×8행 빈 영역에서 세로 I만 고려해도, 각 열의 8개 원본
행을 두 개의 4행 집합으로 분할하는 방법은 `C(8,4)/2=35`개다. 역행 삭제
표현은 연속하지 않은 원본 행의 I도 포함하므로 열별 선택이 독립적이면
raw exact-cover는 적어도 **35^8=2,251,875,390,625개**다. 이것은 추가
pruning 전 원시 배치 집합의 수이며, 실제 실행 가능한 해 수나 최종
enumerator가 방출하는 후보 수를 뜻하지 않는다.

같은 I라도 위치와 삭제행 의존성이 다르다. 큐가 한 가지라는 이유로 이
배치들을 `16!`로 나누거나 동일 operation으로 취급할 수 없다.

## 5. 기존 PC 알고리즘의 Big O와 개선의 위치

기호: k=목표 배치 수, B=WH=목표 셀 수, R=역투영 realization 수,
S=서로 다른 skeleton 수, P=방문한 residual domain 상태 수,
C=명시적으로 검산할 기하 후보 수, G=대상 미노 개수 벡터 수.

표준 tetromino가 최대 4개 원본 행을 선택하므로 raw catalog record 수의
상한은 `R=O(W H^4)`다. 현재 sorting/grouping까지 포함하면 안전한 상한은
`O(R log R)`다. 현재 고정 크기 carrier를 무한 높이의 상수 비용 보드로
간주하지 않으며, 큰 dense mask로 일반화하면 word 수 비용도 추가해야 한다.

현재 extended domain의 안전한 상태당 상한은 `O(S+B^2+Gk)`이고,
전체 domain 작업은 `O(P(S+B^2+Gk))`다. 상태당 다항식이라는 사실로
P까지 다항식이라고 결론 내릴 수 없다. memoization이 있어도 원시 결과 수와
서로 다른 residual 상태 수는 별개다.

기존 전체 비용은 다음처럼 나누는 편이 유용하다.

\[
T=T_{catalog}+T_{family}
  +\sum_{g\in\text{검산 후보}}(T_{physical}(g)+T_{source}(g))
  +T_{result}.
\]

고정 candidate의 operation subset DP에는 최대 `2^k`개 subset과 상태당
최대 k개의 다음 operation이 있다. 고정 movement profile의 pose graph는
`O(WH)`개 상태다. 현재 확장 검산은 subset/child의 보드 투영을 반복하며
물리 순서 그래프를 만든 뒤 공급과 교집합을 계산한다. 따라서 `2^k`는
현재 설계의 상태 공간 상한을 설명하지만 **어떤 알고리즘도 반드시 쓰는
시간 하한은 아니다**. CountAll, 스핀/점수/finesse 증거, queue observation
정책에는 추가 상태와 출력 비용이 있다.

### APDP 중복 제거의 정확한 의미

SameTile이 만든 정확한 full-parent domain을 V(q), static APDP support를
A(q)라 할 때, 기존 분기는 이미 `V(q) ⊆ A(q)`를 확인하고 다시
`V(q) ∩ A(q)`를 계산했다. 이 분기의 결과는 V(q) 그대로다.
`geometry_domain.rs`와 `extended_geometry_domain.rs`의 중복 recheck 제거는
이 항등식을 이용한다. APDP 전체 제거도 새 pruning도 아니다.

P7P4 완료 fixture는 advanced APDP parent-recheck domain 호출이 **0**이다.
따라서 이 fixture의 14–16초 편차를 그 변경의 성능 이득으로 설명할 수 없다.
기존 소규모 full-parent differential test는 결과 동등성을 뒷받침하지만
일반 입력의 실행 시간 개선을 대신 증명하지는 않는다.

## 6. 데미지·스핀 forward 경로

`clearra-forward-search`는 단순히 모든 순서를 끝까지 전개하는 DFS가 아니다.
`StateKey=(board,active,cursor,hold,combo,back_to_back)`와 layer 상태 병합,
trace parent 공유를 사용한다. `DamageAtLeast`에서는 누적 공격량도 state
index에 들어간다.

| 모드 | 상태 처리 | 현재 결과 계약 |
| --- | --- | --- |
| MaximumDamage | 같은 미래 상태에서 작은 공격량 제거, 동률 trace 보존 | 모든 최대 동률 경로 명시 복원 |
| DamageAtLeast | 공격량까지 동일한 상태 병합 | 기준 이상 경로 명시 복원 |
| SpinFinder | 공격량을 상태 구분에서 제외, witness trace 보존 | 스핀 identity별 대표 경로와 정확한 witness 수 |
| MaximumRen | 의미 상태 병합 | 최대 연속 클리어 동률 경로 복원 |

V=방문한 의미 상태 수, E=lock 전이 수, n=명시적 큐 길이,
Lout=출력 경로 step 총수, Atrace=경로 수 집계가 실제 읽은 parent 참조 수라
두면, 현재 비용은 대략 다음 요소로 구성된다.

\[
T_{forward}=T_{reachability}(V)+T_{transition}(E)
 +T_{repeated\ suffix}(V,E,n)+T_{trace}(Atrace)+T_{output}(Lout).
\]

고정 kick table에서 movement pose graph는 `O(WH)` 상태다. 하지만 같은
board라도 공급 cursor, Hold, combo, B2B가 다르면 이동 계산을 다시 한다.
Pattern 입력은 concrete queue별로 새 session을 만들기 때문에 다른 큐의
공통 prefix를 자동으로 공유하지 않는다. 이 비용을 하나의 `b^n`으로만
기술하면 실제 최적화 대상을 놓친다.

### 현재 코드에서 확인한 축약 후보

1. **T 잔여 공급 조회.** `t_spin_acceleration.rs`는 매번
   `queue[cursor..].contains(T)`를 계산한다. 큐당 마지막 T 위치를 O(n)에
   한 번 구하면 각 조회는 O(1)이다. active/hold의 T 검사는 유지한다.
2. **Forward 전용 B2B 의미 상태.** 현 `TetrioDamageProfile::evaluate`의
   미래 공격량은 B2B 횟수가 아니라 `is_some()`에만 의존한다. Forward
   path/outcome은 수치 B2B를 노출하지 않으므로 `None`/`Some(_)` 두 등가류로
   내부 state key를 합칠 근거가 있다. 공용 scorer의 B2B 수치 API는 바꾸지
   않고, 모든 관련 출력·제약을 비교하는 작은 differential 검증이 필요하다.
3. **보드별 이동 결과 재사용.** `(board,piece,evidence mode)` 결과 cache를
   제한 용량으로 검토한다. evidence mode에는 `retain_rotation_evidence`와
   `measure_immobility`를 모두 포함하고 height/rule은 request owner에 결속한다.
   `clearra-spin-structure-search/src/structural_verify.rs`의 bounded
   `EntryCacheKey`와 기존 4,096-entry cache를 먼저 비교해야 한다.
4. **trace parent-chain 합 재사용.** `trace_path_counts`가 여러 child의
   동일 parent chain 합을 반복한다. layer가 봉인되어 immutable이 된 뒤
   그 합을 공유할 수 있다. 생성 중 chain에 적용하면 누락 위험이 있다.

이는 구현 가능성 평가이며 측정된 속도 향상 주장이 아니다. cache는 hit rate와
복사/메모리 비용을 확인해야 하고, 출력 계약을 줄이는 변경은 별도로 다뤄야 한다.

## 7. 현재 알고리즘과 무관한 이론상 하한

### 고정 10×24 보드에서 최대값/존재 여부

고정 W,H와 규칙에서 가능한 물리 보드 수는 유한하다. 현재 공격량 계산의
B2B를 존재 여부로 표현하고 combo의 물리적 한계를 사용하면 미래 점수 상태도
유한해진다. 따라서 명시적 단일 큐의 최대값/존재 여부는 이론적으로
`O(n C(W,H))` word-RAM 연산의 유한 상태 DP로 계산할 수 있다.
C(W,H)는 대략 `2^(WH)`급 보드 상태를 포함하므로 현실적으로 작다는 뜻은 아니다.
총 공격량의 정수 bit 길이를 비용에 포함하면 그 산술 비용도 곱해야 한다.

combo가 보드 크기에 의해 제한되는 이유는 한 번의 연속 clear가 4셀을
추가하면서 최소 W셀을 제거하기 때문이다. 정규화된 보드의 점유 셀 수는
최대 `(W-1)H`이므로 W>4에서 연속 clear 수는

\[
c\le\left\lfloor\frac{(W-1)H}{W-4}\right\rfloor.
\]

W=10,H=24이면 36이다. 사용자 initial combo는 첫 연속 구간의 offset
상태를 추가하지만 무한한 새 축을 만들지 않는다. 현재 u16 큐 길이 제한을
그대로 두면 입력 가족도 유한하므로 n에 대한 점근 분석은 자연스럽게
길이와 누적 점수의 정수 표현을 함께 확장한 모델임을 명시한다.

### 모든 동률 경로 출력에는 실제 지수 하한이 있다

현재 CLI/core가 허용하는 빈 10×2 보드, Hold off, spin disabled 또는
T-spin profile, O를 5m개 공급하는 입력을 생각한다. O 다섯 개를 다섯
2열 자리로 넣는 순서는 `5!`가지이며 모두 harddrop 가능하다. 마지막 O에서
2행 PC가 되어 빈 보드로 돌아간다. 높이 2에서는 다른 O를 위에 쌓거나
더 높은 행 클리어를 할 수 없다. 현 scorer는 PC마다 공격량 5를 준다.

현재 지원 큐 길이 범위에서 최대 공격량은 5m이고 서로 다른 최대 경로가
적어도 `(5!)^m`개다. 점근 분석에서는 위와 같이 누적 점수 표현도 확장한다
(u32 포화 연산을 그대로 고정하면 공격량은 `min(5m,u32::MAX)`다).
현재 `DamagePathKey`는 각 placement 좌표를 구별하므로 이 경로들은 서로
다른 출력이다. n=5m일 때 모든 경로를 명시하는 계약의 출력 하한은

\[
\Omega\bigl(n(5!)^{n/5}\bigr).
\]

이것은 소스 계약에서 도출한 입력족의 증명이며 벤치마크 결과가 아니다.
최대 공격량 숫자 하나, 대표 경로 하나에는 이 지수 하한을 적용할 수 없다.
SpinFinder도 모든 witness를 나열하지 않고 큰 정수로 count하므로
`Ω(witness 수)` 하한이 생기지 않는다. 필요한 것은 count 숫자의 bit 길이와
대표 경로/identity 출력량이다.

### 입력과 출력 계약별 구분

| 문제 | 적용할 수 있는 하한/결론 | 적용할 수 없는 주장 |
| --- | --- | --- |
| 명시적 큐의 정확한 최대값·존재 여부 | 일반 최악 입력의 읽기 비용 Ω(n), 출력 길이 | 현 subset 개수만으로 Ω(2^k) |
| 모든 해/모든 최대 경로 출력 | Ω(전체 명시 출력 길이) | 해 하나를 구하는 문제에도 같은 하한 |
| 압축 공급에서 scalar 확률 | 입력 설명/정확 숫자 표현 크기 | 큐가 Q개라는 이유만으로 Ω(Q) |
| 각 큐별 결과를 개별 출력 | 실제 큐별 출력량 | 압축된 확률 결과와 같은 비용 계약 |
| 가변 보드의 일반 PC/survival | 해당 변형의 NP-hardness, P≠NP라면 일반 다항 해법 불가 | NP-hardness만으로 2^Ω(n) 또는 특정 초 단위 하한 |
| 고정 candidate 물리 판정 | 읽기/증거 출력 하한, 제한된 certificate 사례의 더 빠른 알고리즘 | 현재 2^k subset DP가 최적이라는 결론 |

### 1차 연구와 적용 조건

- Asif 외, [Tetris is NP-hard even with O(1) rows or columns](https://erikdemaine.org/papers/ThinTetris_JIP/),
  JIP 28, 2020. 제4.1정리는 c≥8 고정 열 수·가변 높이·임의 초기판의
  survival/clearing을 다룬다. 회전 가정에 SRS가 포함되며 Hold는 해당 모델에
  없다. 양 축이 고정된 10×24나 빈 보드 damage/spin의 정리가 아니다.
- MIT Hardness Group 외,
  [Tetris Is Hard with Just One Piece Type](https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.FUN.2026.32),
  FUN 2026. O를 제외한 단일 종류로도 SRS의 survival/clearing이 NP-hard다.
  보드 크기와 임의 초기판이 입력의 일부이며, 같은 종류라 Hold가 난해성을
  없애지 않는다. Clearra SRS+의 추가 180도 회전, sky entry, damage/spin
  목표로 그대로 전이하는 정리는 아니다.
- MIT Hardness Group 외,
  [Tetris with Few Piece Types](https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.FUN.2024.24),
  FUN 2024. SRS 두 종류 제한 clearing의 NP/#P-hardness를 다룬다.
  고정 보드 SpinFinder의 압축 경로 수 집계에 바로 적용할 수 없다.

NP-hardness는 특정 입력들의 실행 시간을 모두 지수라고 판정하는 도구가
아니다. 2026 논문의 1×k 조각 긍정 결과는 `H≥2k-1`, 상단 `k-1`행이 빈
보드라는 조건의 무한 공급 생존/언젠가 PC 판정이다. 정확히 n개를 쓰는 현재 fixture를 해결하는
정리로 바꾸어 읽지 않는다.

### 고정 candidate의 제한된 다항 판정 가능성

충분한 certificate가 합법 operation 순서를 정확히 precedence DAG의
위상정렬들로 나타낼 수 있음을 증명하고 공급 순서가 자유롭다면, 존재 판정은
certificate 이후 `O(k+e)`로 가능하다. 총비용은 `T_certificate+O(k+e)`다.
현재 `placement_poset.rs` certificate compiler도 삭제행 mask를 열거하므로
일반 높이에서 certificate 생성 자체가 다항이라는 보장은 없다. 선행조건이
필요조건이라는 사실만으로 충분조건 certificate를 얻었다고 할 수 없다.

고정 개수 chain의 정확한 분해가 주어지면 `∏(chain length+1)` 진행 상태를
사용하는 다항 DP도 가능하다. 실제 큐/Hold 상태와 교집합 비용은 포함해야 한다.
ordered kick의 앞선 kick 실패 조건, support/collision의 음성 조건 때문에
일반 candidate를 단순 위상정렬로 대체할 근거는 없다.

## 8. 별도 spin-structure 경로

`clearra-spin-structure-search`는 forward SpinFinder와 다른 엔진이다.
operation catalog, 목표 operation 분할, clear-row 부분집합, fill exact cover,
support/blocker/roof 확장, 실제 movement 검산, subset-minimality의 순서다.

가변 clear-row 수 r, 최대 placement 수 k일 때 `2^r` clear-row
선택과 placement 선택이 결합한다. `VerificationCursor`는 remaining
operations·logical board·deleted rows의 실패 상태를 memo한다.
`MinimumCursor`의 같은 target/clear-row group 내 subset 비교는 group g에
들어오는 필터 전 후보 수를 N_g라 할 때 `Σ_g O(N_g^2 k)` 비용을 가질 수
있다. 최종 출력이 작아도 이 비용은 클 수 있다. 목표 간 중복 검산과 minimality 비교를
각각 계측해야 하며 forward의 분기식 하나로 대체할 수 없다.

## 9. 개선 우선순위와 필요한 확인

### 지정 필드 경로

1. **동적 자원 보고 구분.** digest reservation의 branch count와 bytes는
   위 재현으로 확인했다. ResourceReport의 실행 시작/실패 phase 표현을 분리한다.
2. **digest 저장을 후보 수와 분리.** ordinal chunk와 제한된 in-flight
   window, 완료된 연속 prefix folding으로 `Θ(C)` 저장을 `O(pq)`로 줄이는
   설계를 검토한다(p=worker 수, q=chunk 후보 수). `reserve_exact`만 없애거나
   모든 작은 segment를 완료까지 보존하면 전체 Θ(C)는 사라지지 않는다.
   FNV 순서를 임의 XOR/합으로 바꾸지 않는다. 요청 스레드는 계산에 참여하지 않는다.
   보조 메모리를 줄여도 수백억~수조 후보의 물리 검산 시간은 남는다. 이 변경만으로
   큰 입력이 실용적으로 완주한다고 주장할 수 없다.
3. **단일 큐·CountUnique의 후보별 검산 축소.** compact witness 경로의
   자격을 바탕으로 extended에서도 공급과 물리 조건을 함께 탐색해 필요한
   성공 증거만 얻을 수 있는지 검토한다. CountAll, 전체 coverage, 최소 ordinal,
   spin/finesse 증거는 별도 계약이다. family DAG 직접 풀이 제안이 아니다.
4. **검산 재사용.** `extended_buildup.rs::state_for_subset`의 같은 child
   projection 반복과 `extended_reachability.rs`의 같은 board/piece 결과
   반복을 계측한다. subset 내부 piece별 lock 재사용은 이미 있으므로
   후보/다른 subset 사이의 반복만 새 개선 대상으로 본다.
5. **큰 prefix 증거 공유.** compact가 생략하는 부정 prefix 증거를 확장
   경로에서 재사용할 수 있는지 검토한다. 64비트·6행·11미노의 기존
   certificate 가정을 검증 없이 넓히지 않는다.

### forward 및 구조 스핀 경로

T suffix 조회와 Forward B2B 등가류를 먼저 검증할 가치가 있다. 이어서
bounded movement-result cache, 봉인된 trace 합의 공유, pattern 공통 prefix,
구조 스핀 minimality index를 실제 발생 횟수와 비용으로 순위화한다.

최대값/대표 하나와 모든 경로 출력의 계약을 명시적으로 구별하면 출력
폭증을 피할 수 있는 범위가 넓어진다. 기존 모든 경로 결과를 조용히 대표
하나로 바꾸는 것은 최적화가 아니라 기능 변경이므로 이 분석에서 수행하지 않는다.

새 무거운 6L fixture를 CI에 추가하지 않는다. 작은 fixture의 ordinal/digest/
해법·coverage 동등성, chunk 완료 순서 변화, 취소, 메모리 경계 검증으로
구조 변경의 정확성을 확인하고, 성능 확인은 감독된 별도 진단으로 수행한다.

## 10. 이번 변경과 검증 범위

이번 소스 변경은 실패 발생 지점을 관측하는 선택적 진단뿐이다. 최적화 후보를
제품에 도입하거나 결과 개수·출력 의미를 변경하지 않았다. 이전 중단 시점의
`fixed_field_cost_bench.rs`도 함께 검토해 재사용했다.

Rust 1.98.1의 locked/offline release 빌드를 완료했다. I16/I18의 짧은
감독 실행으로 같은 ResourceAdmission과 정확한 실패 지점을 재현했고,
요청 스레드 비참여·9 CPU 예산·종료 후 descendant 0을 확인했다. 기존 완료
자료의 비율은 Rust 재계산에서 원래 JSON과 대조했다. `git diff --check`를
통과했다. 최적화 구현의 성능/정확성 qualification, 전체 제품 배포와 CI
matrix 실행은 이 분석의 결과로 주장하지 않는다.

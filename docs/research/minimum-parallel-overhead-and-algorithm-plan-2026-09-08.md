# 최소 해법: 병렬 비용의 원인과 알고리즘 A/B 후보 재정리

> 2026-09-10 추가 후속: 아래 **6.1–6.3 전체 후보**를 다시 평가했다.
> T/F, assistance 대조, reasoned PB/CDCL 및 incremental assumption scout,
> reference 설정 대조와 실제 4195 batch 결과는
> [전체 후보 평가와 장비별 계측](minimum-sections6-evaluation-2026-09-10.md)에 있다.
> T+F를 성능 브랜치 일반 설정에 적용하고, 낮은 병렬도에서 회귀한 assistance off는
> 독립 A/B로 남겼다. 11 worker 결과를 다른 PC의 기본 정책으로 일반화하지 않는다.

> 2026-09-10 후속: H와 O1의 남은 후보 수 기반 분할 예산을 독립 A/B하고,
> 이득이 재현된 조합을 성능 브랜치 기본값으로 적용했다. V0의 profiling
> 소유권 충돌도 수정했다. [실측·검증·남은 한계](minimum-impact-and-partition-ab-2026-09-10.md)를
> 현재 상태로 읽고, 아래의 미구현·미검증 표기는 9월 8일 분석 당시 기록으로 구분한다.

## 0. 상태와 결론

이 문서는 2026-09-08 사용자의 **소스 수준 분석 요청**에 대한 후속 기록이다.
기존 `v080-minimum-algorithm-hotfix-ab-2026-09-07.md`의 제품 의미, local-only,
SRP, 취소/메모리/증명 경계는 그대로 유지한다. 그 문서의 과거 실행 ID를 현재
배포 상태로 해석하지 않는다. 이 문서는 새로운 실행 또는 배포 승인 증거가 아니다.

- Qnia 2워커가 11워커보다 빠른 주된 관측 근거는 **동일 작업의 고정 분할이
  아니라 full solver 복제로 총 증명 작업량이 증가한다는 것**이다. mutex/캐시/
  메모리 대역폭 대기 비중까지 실측한 것은 아니다.
- Clearra의 이번 표본은 2워커가 11워커보다 느렸다. 다만 작은 canonical 후반
  질의는 11워커에서 더 오래 걸렸다. 전역 2워커 또는 전역 n워커 강제는 결론이 아니다.
- Clearra에는 분할마다 재준비하는 검색 상태와, canonical 질의 사이에서 버리는
  추론 상태가 있다. 전송 비용 절감과 **증명 작업 자체의 감소**를 별도 후보로 둔다.
- Qnia의 최소 개수 K를 부정확하다고 기각할 근거는 없다. 그러나 Qnia Fast의
  임의 optimal witness는 Clearra의 정확한 첫 original-ID canonical 집합과 다르다.
- 3초 목표를 달성하려면 K 증명과 canonical 증명 모두 개선해야 한다. 현재
  근거로 워커 수/시작 비용만 바꾸면 3초에 도달한다고 약속할 수 없다.

이번 소스 감사에서는 세 개의 읽기 전용 서브에이전트로 Qnia, Clearra, 기존 후보
목록을 분담했다. 분석 요청 이후 새 벤치마크·빌드·WASM 교체·CI 결과 조회는
실행하지 않았다. H 후보는 앞서 작성한 **미빌드·미검증·미커밋 draft** 그대로다.
이 문서와 기존 계획의 연결만 추가하며, 구현 선택은 다음 단계에서 확정한다.

## 1. 비교 계약과 원시 근거

공통 입력은 `ctk3_w0kCQBjwwAMPPAD37g`, P7, 4L, hold, **Jstris 180**이다.
원문 행렬은 5,040 queues × 246 candidates, SHA-256
`63de33e1d86077c179a38f6311df893ba9abcc13c9b18fa433384f5961eeee91`이다.
Qnia 생성 결과와 Clearra 행렬의 모든 candidate/queue incidence 차이는 0이었다.
Qnia cardinality kernel은 1,389 constraints × 158 candidates이다.
25는 출력 사후 조건이며 solver의 입력 bound/hint로 주입하지 않았다.

| 대상 | 고정한 기준 |
| --- | --- |
| Qnia | `03b637730c5b541f4f2934be613498fbe65327fd` |
| Qnia OR-Tools | 공식 `551ad10d94835c99e5e1e684500d3db398c0e345`, v9.15.6755 |
| Qnia WASM port | `a16c07886b1db846248a477ed5c06ba93c484493` |
| Clearra 연구 브랜치 | `codex/v0.8.0-hotfix-minimum-algorithm-ab`, HEAD `2e9cdd3095412523903a5df59a8c701d682736a6` |
| Clearra 실행 자산 | local factorial-v4 WASM `ccb8f6f07b1234dae1f4c9689f91b9d8c79b84032ef0e56b6dc927dd0e5fa849`; H 미포함 |
| 실행 환경 | 같은 로컬 desktop, 12 logical processors, Node 24.16.0 WASM; **실제 GUI 또는 모바일 측정 아님** |

원시 소스/외부 solver/행렬/실행기는 root checkout의 `_local/`에만 존재한다.
이 요약 이외 원시 자료를 CI, Docker, Pages, CLI 배포 패키지에 포함하지 않는다.

- Qnia: `_local/reports/qnia-stage-correctness-1788862232596.json`
- 독립 원문 검증: `_local/reports/qnia-independent-proof-1788862520641.json`
- Clearra W11: `_local/reports/minimum-product-node-ab-1788862373190.json`
- Clearra W2: `_local/reports/minimum-product-node-ab-1788862623392.json`
- Clearra W4: `_local/reports/minimum-product-node-ab-1788862659582.json`
- Clearra W1 실패: `_local/reports/minimum-product-node-ab-1788862686758.json`

각 실행은 직렬로 진행했고 벤치마크/빌드를 서로 겹치지 않았다. 다만 전용으로
격리한 CPU 실험 환경은 아니며, Qnia는 조건당 2개, Clearra는 1개의 유효 표본이다.
전역 최적 워커 수나 유의한 p90을 추정할 표본은 아니다. 이전 SRS+ 행렬 실험을
이 Jstris 행렬의 표본으로 합치지 않는다.

## 2. 구간별 관측

### 2.1 Qnia: 순수 CP-SAT와 제품 부가 구간

`solver`는 CpSolver의 wall time이다. solver 내부 presolve/search를 포함하며,
모듈 다운로드 시간 또는 Clearra canonical 증명을 포함하지 않는다.

| workers | full max_lp 개수 | solver 초, 두 표본 | 총 deterministic work | full solver LP iteration 합계 |
| --- | ---: | --- | --- | --- |
| 1 | 1 | 11.612 / 15.055 | 18.590 / 18.590 | 384,300 / 384,300 |
| 2 | 1 | 3.119 / 4.830 | 8.113 / 8.237 | 96,553 / 96,553 |
| 4 | 3 | 3.479 / 6.440 | 17.309 / 23.715 | 282,221 / 389,775 |
| 11 | 8 | 6.475 / 6.479 | 57.085 / 51.520 | 853,387 / 785,848 |

2워커 두 실행의 feature 구간은 다음과 같다. 별도 initial WASM load는 약 13/22ms이다.

| 구간 | 첫 실행 | 두 번째 실행 |
| --- | ---: | ---: |
| 후보 열거 | 0.069s | 0.137s |
| coverage 수집 | 0.048s | 0.094s |
| numeric 변환 | 0.00002s | 0.00003s |
| kernel | 0.012s | 0.028s |
| primary 전체: worker/module/model/solve 등 | 3.352s | 5.172s |
| 위 primary 중 CP-SAT solver | 3.119s | 4.830s |
| Fast quality + 결과 출력 | 0.308s | 0.301s |
| feature 전체, initial load 제외 | 3.789s | 5.733s |

11워커 primary 바깥 추가 비용은 약 230/235ms로 2워커 233/342ms보다 크지 않았다.
따라서 이 표본의 11워커 지연을 worker 생성/모듈 로드만으로 설명할 수 없다.
25개 feasible witness는 2워커 0.63/0.92초, 11워커 1.02/1.40초에 도착했다.
그 뒤 최적성 증명 완료까지 각각 2.49/3.91초와 5.46/5.08초가 더 걸렸다.

1워커 두 실행은 conflicts/branches/LP iterations 및 deterministic work가 같아도
wall이 달랐다. 2워커의 full max_lp도 두 번 모두 1,145 conflicts, 1,895 branches,
1 restart, 96,553 LP iterations였다. 실행 속도 편차와 검색 작업량 편차를 나눠야
한다. thermal/cache/OS scheduling 중 어느 원인인지는 이 자료로 특정하지 않는다.

### 2.2 Clearra: 첫 canonical 집합까지

Clearra는 실제 제품 JS worker graph와 local WASM을 Node host에 연결했다.
prewarm은 run timer 밖이며, 첫 집합까지의 elapsed에는 K뿐 아니라 canonical이
포함된다. 아래 selector 경계는 wire의 5,040→5,041 constraint 전환과 소스의
추가 selector bit 구성으로 확인했다. **정밀 K-only timer가 아니라 구간 경계**다.

| 구간/작업 수 | W2 | W4 | W11 |
| --- | ---: | ---: | ---: |
| 전체 elapsed | 35.914s | 26.916s | 28.992s |
| 시작→첫 canonical selector query | 18.431s | 13.889s | 12.948s |
| 이후→첫 집합 terminal | 17.484s | 13.027s | 16.044s |
| exact waves | 29 | 29 | 30 |
| 원격 exact task 합계 | 200 | 414 | 940 |
| query_prepare 합계 | 48.6ms | 47.9ms | 97.7ms |
| upstream_gap 합계 | 332.6ms | 422.1ms | 1015.9ms |
| rows<132인 첫 canonical suffix→terminal | 444.1ms | 694.0ms | 2221.8ms |

W11에서는 finish_start가 약 0.161초, 최초 K query가 0.247초에 있었다. query limit은
27→26→25→24 순으로 내려가며 K−1 부정증명을 완성했다. selector 진입 이후에도
약 6.715초와 3.874초가 걸리는 큰 질의가 각각 남았다. 따라서 K 이후 비용은
단순 UI 렌더 또는 전체 동률 열거 비용이 아니라 **첫 canonical 집합의 증명 비용**이다.

W11의 source 약 0.098초, candidate drain 약 0.004초, verifier finish 약 0.044초는
수십 초의 minimum 단계와 분리한다. Geometry/legal-board 개선만으로 현재 minimum
격차를 해결할 근거는 없다. W4의 26.916초를 실제 GUI 3초 성능으로 해석하지 않는다.

### 2.3 실패 표본과 계측 함정

- W1은 7.8579ms에 start가 실패했다. 성공한 성능 표본에 포함하지 않는다.
- 정적 소스에서 `WasmJobRunner.profile_start()`→`state.profile=Some`→
  `has_worker_job_start_conflict()`→오류 텍스트 없는 0 반환이 확인된다.
  **stage-profiling serial start의 소유권 충돌**이 관측과 일치하는 가장 강한 원인이다.
  수정·재실행 및 실행 artifact의 완전 provenance 재검증 전이므로 일반 production
  브라우저 버그 또는 Node 고유 버그라고 확대하지 않는다. 다음 W1 비교의 선행 수정이다.
- Node host는 IndexedDB를 주입하지 않아 MemoryDelegationJournal을 사용한다.
  위 자료는 실제 GUI IndexedDB commit 지연을 측정하지 않았다.
- `remote_admission_wait_ms`에는 이미 일하는 worker의 탐색/완료 대기도 포함된다.
  이를 전부 관리자 overhead로 세지 않는다. transport timer는 서로 중첩된다.
- ready subset만으로 종료한 wave에서는 늦은 all-ready callback이 wave 완료 뒤에
  기록될 수 있다. 35.2ms wave의 all-ready 84.9ms를 blocking 비용으로 합산하지 않는다.
- Qnia `searchedStates=cp.numBranches`는 병렬 전체 탐색량이 아니다. 11워커 summary의
  branches=474, LP iterations=0만 보면 실제 수십만 LP 반복을 놓친다.

## 3. 정확성: 어떤 문제를 증명했는가

각 후보 j에 Boolean x_j를 두고, 각 큐 i에 대해 `sum(A_ij*x_j) >= 1`, 목적은
`min sum(x_j)`이다. 이는 현재 입력의 minimum set cover 정의와 일치한다.

K 증명용 축약은 다음 이유로 최소 개수를 보존한다.

1. 어떤 큐의 supporter가 하나면 그 후보를 강제하고 커버된 큐를 제거할 수 있다.
2. supporter 집합 A가 B의 부분집합이면 A 커버 제약이 B를 함의하므로 B는 중복이다.
3. 후보 a가 커버하는 큐가 b의 부분집합이면 a를 b로 대체해 선택 수를 늘리지 않는다.
   동일 coverage 후보도 cardinality 목적에서 대표 하나로 축약할 수 있다.

단, 3은 **모든 optimal 집합이나 original-ID lexicographic 첫 집합을 보존하지 않는다.**
예컨대 후보 `{a}`, `{a,b}`, `{b,c}`, `{c}`의 K=2 첫 집합은 `[0,2]`다. dominated
후보 0을 지우면 K는 같아도 첫 집합은 달라진다. proof-only kernel과 원본 canonical
행렬을 분리해야 한다. Qnia Fast secondary가 exact가 아니라는 이유로 K를 부정하지 않는다.

이번 근거는 다음 수준이다.

- Qnia 8회 모두 `OPTIMAL`, objective=bound=25, 원문 5,040큐 커버 재검증 성공.
- 4개 후보의 공집합이 아닌 supporter 15종으로 만들 수 있는 제약 집합 32,768개
  (빈 제약 집합 포함)에 대해 kernel의 minimum-K 보존을 brute force로 대조했다.
  무한 입력 전체의 형식 증명은 아니다.
- 원문 246후보/5,040제약 그대로 별도 HiGHS WASM에 `sum x <= 24`를 물어
  `Infeasible`을 확인했다(20.174초). 이는 독립적인 교차 확인이며 속도 비교 후보가 아니다.
- 원문 LP relaxation은 20.9323854895539. 별도 BigInt 검산한 rational dual은
  `20932387327 / 1000001599`, 올림 하한 **21**만 증명한다. 이를 25의 인증서라고
  부르지 않는다. LP를 같은 완화에서 더 정확히 푸는 것만으로 K=25를 증명할 수 없다.
- CP-SAT 로그는 `LRAT_status: NA`다. 두 exact solver의 일치, 모델/원본 커버 검증은
  강한 근거지만 독립 checker로 검증한 K=25의 LRAT/DRAT proof trace는 없다.

결론: 이 입력에서 Qnia의 정확한 K를 기각할 이유가 없어 기존 3초 P2 목표는 유지한다.
단순히 Qnia witness를 반환해 Clearra의 canonical 추가 의미를 없애는 변경은 하지 않는다.

## 4. 병렬 비용의 소스 원인과 해결 가능성

### 4.1 Qnia / OR-Tools

Qnia는 `subsolvers=['max_lp']`, seed=1을 지정한다. OR-Tools의 자동 full solver 수는
2/4/11 workers에서 1/3/8개이며, 목록이 부족하면 첫 파라미터를 그대로 복제한다.
이 단계에서 seed도 동일하다. 이후 공유 정보 도착과 비결정적 스케줄에 따라 궤적이
달라지는 **중복 portfolio**이며, 같은 트리를 8조각으로 분담하는 shared-tree가 아니다.
11워커의 LP 반복 총합은 2워커의 8.14~8.84배다. [공식 full solver 구성][O1],
[복제된 parameter의 local model 대입][O2].

2워커에서는 FJ가 초기 feasible 해를 공급하고 이후 LS/LNS 등이 incumbent를 개선한다.
1워커에서는 등록된 LNS가 실제 0회였으므로 1→2 이득도 단순 병렬 속도만이 아니다.
11워커 first-solution solver 셋은 첫 해 뒤 종료되므로 전 구간 내내 8+3 고정 실행이라고
표현하지 않는다. 목적/하한/학습 절 공유와 task scheduler에는 잠금이 있지만, 이 잠금의
wall 비중은 미측정이다. [scheduler][O3], [clause 공유][O4].

해결 후보는 full solver 개수 고정, 명시적 다양화, 검증한 incumbent 조기 전달,
shared-tree 대조다. 지속 Worker는 약 수백 ms의 부가 비용 후보이며 증명 알고리즘
수초의 차이를 그 자체로 없애는 대안은 아니다. 아무 subsolver나 늘리거나 공유를 모두
끄는 것은 해결책으로 확정하지 않는다.

`T(p)`는 관리 비용뿐 아니라 p에 따라 달라지는 **실제 탐색량 W(p)**, 가용 실행속도,
증명 critical path의 함수다. 같은 W라는 조건이 깨지므로 이번 수치에 고정-workload
Amdahl 식을 대입해 serial 비율이나 11워커 이론 성능을 역산하지 않는다.

### 4.2 Clearra

| 위치 | 확인한 비용 | 해결 후보와 경계 |
| --- | --- | --- |
| root partition | worker 수에 따른 cube 개수/가정 변경으로 총 DFS work도 변경 | fanout과 worker 수를 별도 제어. H는 root 순서만 변경 |
| shard start | residual/remap/hint, dense reduction, constraint quotient, root dual/search workspace를 각 shard에서 준비 | immutable base와 가역 query overlay 분리. proposal 재공유 시 residual에 재인증 |
| memo | 현재 excluded_rows가 전부 0일 때만 covered-state memo 사용 | exclusion/assumption scope-aware memo 또는 reasoned learning. 기존 키로 전역 활성화 금지 |
| idle assistance | 원래 cursor는 유지하고 complete child cubes를 다시 경주시킴 | 중복 work 제한 또는 미방문 subtree의 실제 donation. 누락/이중 소유 없는 proof closure 필요 |
| canonical | prefix/selector마다 새 ExactLexQuery/AtMost oracle 및 generation 생성 | 동일 원본 matrix/fixed K의 incremental assumptions와 학습 재사용(B2) |
| remote wave | worker마다 payload slice/hash/decode와 admission/receipt lifecycle 반복 | matrix session과 query delta, 작은 suffix의 fanout 축소. guard/fence/취소 증거 유지 |
| controller | 준비된 worker부터 시작하지만 admission/authority transition은 직렬 관리 | blocked/idle/compute를 별도 측정. 무조건 controller compute 참여로 바꾸지 않음 |

같은 WASM 인스턴스의 Arc query/bitset은 공유된다. `row.clone()`을 모두 전체 bitset
복사라고 계산하지 않는다. 원격 worker도 한 wave 안에서는 query를 재사용한다.
반면 다음 wave에서는 query를 reset/decode하므로 **Worker/module 재사용과 solver
추론 재사용은 별개**다. 이전 slice의 DFS를 이어가는 continuation은 이미 구현되어 있다.

원자적 terminal pair와 매번 전체 journal history snapshot 복사 제거도 이미 구현됐다.
이를 신규 최적화로 다시 제안하지 않는다. unguarded→guarded 이중 encode, 루프 내
retained-capacity 재합산도 보이지만 이번 알고리즘 우선 작업과 분리하며 주원인으로
단정하지 않는다. 실제 peak-memory 검사를 제거하는 방향으로 고치지 않는다.

## 5. Clearra와 Qnia의 알고리즘 차이 및 적용 판단

| 축 | Qnia 현재 경로 | Clearra 현재 경로 | 적용 판단 |
| --- | --- | --- | --- |
| Geometry | Qnia PC/WASM 후보·coverage | ILC + BuildUp/검증 | 각자 후보 완전성 유지. minimum proof와 별도 |
| K 모델 | kernel 뒤 Boolean set-cover optimization | greedy/repair UB 뒤 반복 AtMost(U−1) exact decision | 목적 동등. U 발견과 U−1 부정증명 시간을 나눠 비교 |
| 추론 | SAT/정수 전파, 이유/학습, LP 기반 탐색·절단 | rarest-support DFS, dominance/quotient, packing/gain 하한, MP dual 정수 검산, 제한적 memo | B1a reasoned PB/SAT와 B1b 더 강한 정수 추론이 큰 차이 |
| 병렬 | full/first-solution/interleaved portfolio, 공유 정보 | exhaustive disjoint shards + bounded repair + idle assistance | Qnia의 worker 상수보다 협업 구조를 비교 |
| 최적성 종료 | objective와 best bound 일치 | 유효 UB + 완결된 U−1 negative receipts | 오류/미완료를 증명으로 바꾸지 않음 |
| 첫 집합 | Fast secondary, 임의 optimal witness 가능 | 원본 ID lex self-reduction | 추가 비용은 B2 대상, 의미 삭제 금지 |
| 다음 동률 | Clearra와 동일한 공개 계약 아님 | first 후 quiescent, next 명시 요청에 lazy | 현재 lazy 계약 유지 |

Qnia가 SAT inprocessing과 zero-half cuts를 껐다고 CDCL/모든 cut이 없는 것은 아니다.
그 옵션만 베끼는 것이 아니라 어떤 유효 추론이 proof work를 줄였는지 비교해야 한다.
Qnia의 `addAssumption` API도 여러 solve 사이 learned clause 보존을 보장하지 않는다.
현재 wrapper는 solve마다 모델을 직렬화하고 disposable Worker를 종료한다.

적용 우선순위는 **B1a/B1b로 K 증명 강화 + B2로 canonical 재증명 감소**다.
과분할·중복 준비를 줄이는 O 후보를 독립 대조한 뒤, 효과가 확인된 축만 조합한다.
자체 구현이 3초 목표에 도달할 근거가 계속 없으면 CP-SAT/HiGHS adapter를 별도 분기한다.
외부 K oracle이 빨라져도 기존 canonical 13~17초를 자동으로 없애지는 않는다.

SRP 경계는 `MinimumModel` / proof-only `CardinalityKernel` / `BoundCertificate` /
`CardinalityProof` / `FixedKAssumptionSession` / `CanonicalEnumerator` / `Scheduler` /
`DiagnosticAdapter`로 유지한다. CLI core가 의미와 증명을 소유하고 GUI와 Discord가
그 위에 올라간다. GUI 전용으로 외부 solver를 붙여 CLI/Discord 의미를 갈라놓지 않는다.

## 6. 이후 A/B 후보 전체 목록 — 확인한 범위의 목록이지 모든 알고리즘 소진 선언은 아님

### 6.1 정확성·진단 선행 및 병렬 실행 축

| ID | 후보 | 현재 상태 / 대조 포인트 |
| --- | --- | --- |
| V0 | profiling serial-start 소유권 충돌·빈 오류 수정 | 소스 원인 확인, 미수정. 다음 W1 유효 비교의 선행 조건 |
| V1 | K/witness/negative closure/canonical purpose별 계측 | 일부 wire 경계만 있음. nested timers를 critical-path interval로 분리 |
| O1 | 작은 suffix에 worker·partition fanout 적응 | 미구현. piece 수 컷 대신 residual 구조와 준비 대비 유효 work를 사용 |
| O2 | immutable matrix session + selector/prefix delta | 미구현. 전송/decode/presolve 재사용과 B2의 학습 재사용을 별도 측정 |
| O3 | assistance 중복 작업 예산·발행 기준 | 기존 assistance 유지 대조. 유효 child closure와 버려진 parent work를 함께 측정 |
| O4 | 실제 미방문 DFS subtree donation / adaptive cube-and-conquer | 현재 one-level racing과 다름. 원래 proof 의무를 빠짐없이 이전하는 프로토콜부터 증명 |
| O5 | shard/root dual proposal 공유와 독립 재인증 | residual warm 기본 true와 다른 축. mutable DFS·denominator 무검증 공유 금지 |
| O6 | 작은 상수 비용: 이중 encode/capacity 합산/관리자 yield | 보조 목록만 유지. 사용자 요청에 따라 알고리즘 A/B에 미세 최적화를 섞지 않음 |

### 6.2 검색·증명 알고리즘 축

| ID | 후보 | 현재 상태 / 정확성 및 중단 조건 |
| --- | --- | --- |
| H | rarest pivot의 root frontier를 residual impact 순으로 분기 | bit 8, 미빌드 draft. exhaustive/disjoint ownership과 first canonical 불변 검증 전 off |
| B1a | reversible trail + OR/PB propagation + reasoned nogood/CDCL | 미구현. assumptions/K/selector scope와 undo/restart 정합성을 먼저 검증 |
| B1a-2 | cardinality encoding 비교: sequential/totalizer/network 또는 native PB | 위와 구분한 하위 가설. equisatisfiability, 증명·전파·메모리 비용 비교 |
| B1a-3 | conflict minimization/activity/restarts, core-guided/MaxSAT 방식 | 후속 가설. 현재 core 단독의 느린 증명 반례를 유지; 빠른 witness만으로 채택 금지 |
| B1b | stronger LP/PB bound, reduced-cost/dual-ray reasoning, 유효 정수 cuts | G보다 넓은 미완료 축. float proposer와 exact checker 분리; coefficient 올림 보존 |
| B1b-2 | graph/clique/odd-cycle/cover·rounded CG 계열 구조적 bound | 해당 incidence에서 유효 구조 확인 후 적용. cut 비용이 절약 work를 넘으면 제외 |
| B2 | fixed-K original matrix의 incremental canonical assumptions | 핵심 미구현. 질의 간 learned/proposal/preparation 재사용; 원본 canonical·lazy tie 불변 |
| B2-2 | incremental objective bound U 감소 및 K proof 상태 재사용 | K−1 oracle 반복을 처음부터 다시 준비하는 비용과 분리. 서로 다른 K의 학습 유효성 증명 |
| B2-3 | 하나의 세션에서 cardinality/lexicographic objective를 순서대로 증명 | 추가 가설. 원본 ID 순서와 tie 보존; 246개 Boolean의 lex weight를 제한된 정수로 근사하거나 overflow시키지 않음 |
| B3 | reversible proof-only presolve / 실제 독립 component별 optimum 합 | 핵심 kernelization은 이미 있음. P7 단일 성분이면 단순 분해는 이 입력에서 제외 |
| B3-2 | separator/cutset, 조건부 분해, canonical-safe symmetry | 추가 가설. 조건별 결합 비용·원본 ID 복원 필요; 임의 mirror/대칭 가정 금지 |
| B4 | scope-safe incumbent/bound/nogood 공유, 다양화된 portfolio | 미구현 부분. disjoint proof와 redundant portfolio를 구분하고 중복 work 포함 비교 |
| U | 더 강한 input-derived incumbent/repair/LNS neighborhood | 여러 greedy/repair가 이미 있음. 25를 힌트로 넣지 않으며 negative proof 단축이 없으면 후순위 |
| F1 | coverage-class/ZDD family, rank/unrank restart, component Product, 1/2-slot kernel | 기존 계획에 남은 별도 버전 후보. lazy family·완전성·재시작을 보존하며 첫 응답 전 전체 count/materialization을 강제하지 않음 |

### 6.3 외부 reference 및 backend 선택 축

| ID | 후보 | 비교 목적 / 경계 |
| --- | --- | --- |
| Q1 | numFullSubsolvers 고정 + workers 1/2/4/8/11 | 동일 max_lp 자동 복제 비용 분리. 실제 full/first/interleaved 구성도 기록 |
| Q2 | 명시적 seed·strategy portfolio; max_lp/default_lp/no_lp/quick_restart/core | 탐색 다양성 대조. 이름만 같거나 seed만 다르다는 이유로 효과 추정 금지 |
| Q3 | input-derived 유효 witness hint, objective decision 분리 | startup/positive/negative 시간을 나눔. 기존 알려진 답의 주입 금지 |
| Q4 | interleave + 고정 batch size, shared-tree, clause 공유 on/off | 과거 interleave 회귀 유지. 결정성 분석용과 성능 후보를 구분 |
| Q5 | same-matrix HiGHS / CP-SAT / 자체 solver | K-proof와 first-canonical endpoint를 별도 대조. OR-Tools assumption API를 자동 incremental backend로 오인 금지 |
| E1 | CP-SAT/HiGHS 제품 adapter | 현재 미도입. native/WASM·취소·메모리·capability·중첩 병렬화·재현 빌드·license/NOTICE까지 별도 승인 판단 |
| L1 | kick별 legal-board 재생성 / negative reachability filter | 별도 geometry 과제. domain 불완전은 unknown, 임의 garbage를 누락 index로 거부 금지; 현재 minimum 직접 해결책 아님 |

외부 프로젝트는 현재 통합 문서에서 Apache-2.0 및 개별 구성요소 고지를 선언한다.
이는 모든 vendor/transitive 파일을 재배포해도 된다는 별도 감사를 대체하지 않는다.
E1 선택 시 port/runtime 및 Eigen 등 구성요소별 고지와 실제 포함 파일을 확인한다.
아이디어만 독립 구현하는 축과 외부 소스/바이너리를 포함하는 축을 구분한다.

### 6.4 이미 있는 것 / 이미 제외한 것을 다시 신규 후보로 세지 않기

- 유지: dominance/duplicate/fixed-point kernel, greedy/randomized repair, validated
  warm witness, residual warm seed(default true), integer-certified conditional prune,
  gain/packing/dual bounds, 질의 내 DFS/memo continuation, ready-worker 즉시 발행.
- M1: global repair 제거는 36.149→82.016초 회귀로 제외. M2는 repair를 bounded
  controller advisory step으로 유지하는 연구 브랜치 후보이며 안정 배포 승격 근거는 아님.
- G: component-rounded support bound는 36.149→40.224초로 현 P7에서 제외/default off.
  이 반례를 B1b 전체의 실패로 확대하지 않는다.
- C: canonical interval bisection은 33.616→51.277초, 29→77 waves로 제외/default off.
  C는 질의 수/분할 전략이고 B2는 질의 간 추론 재사용이므로 B2의 실패가 아니다.
- cached-pivot exhaustion은 별도 default-off 대조군이다. warm seed 조합 추가 이득의
  근거가 부족해 동일 실험을 무조건 반복하지 않는다.
- deterministic interleave의 과거 5.21→15.55초는 회귀다. 새 portfolio/batch 조건을
  명시하지 않고 같은 실험을 재실행하지 않는다.
- 과거 Varisat 0.2.2의 unhinted K=25/K≤24 및 balanced-totalizer K=25,
  BatSat Sinz K=25 probe는 각각 당시 120초 안에 끝나지 않았다. splr 0.17.2는
  panic 또는 높은 메모리/timeout을 보였다. 해당 backend/encoding/당시 입력의
  불채택 근거이며 SAT/CDCL 전체나 B1a/B1b의 영구 기각 근거는 아니다.
- microlp는 opaque allocation을 whole-live memory guard와 WASM abort/OOM 안전성에
  결속하지 못해 제품 도입에서 제외했다. 당시 root probe 하한을 이번 Jstris 원문
  행렬의 K=25 증명으로 재해석하지 않는다.
- residual proposal depth 2–6와 iteration cap 25/50/100/200/400/800도 과거 일부
  비교했다. 알고리즘 변경 없이 같은 상수 튜닝을 반복하지 않는다. 구체 조건은
  [정리 전 통합 계획의 보존 원문][H1] §2에 남아 있다. 이 과거 native/probe 기록은
  이번 실행에서 재검증한 성능 증거가 아니며 현재 local native 실행 금지를 바꾸지 않는다.

## 7. 실행 순서와 채택 기준

1. **V0/V1부터**: W1 실패를 정리하고 source/WASM identity를 고정한다. no-native
   정책은 유지하며 source/Node 계약 및 허용된 Rust→WASM 빌드로 검증한다.
2. 같은 matrix에서 Q1/Q2로 중복 portfolio 가설을 분리한다. Clearra는 O1/O3/H를
   각각 단독 대조한다. 사소한 H를 B1/B2 대신 무기한 반복하지 않는다.
3. B1a와 B1b, B2를 독립 구현·대조한다. B3는 구조 검사부터, B4는 검증된 공유
   단위부터 추가한다. 단독 이득이 확인된 후보만 조합한다.
4. 필요하면 O2/O5로 immutable preparation을 공유한다. 알고리즘 work 감소와
   model/transport 비용 감소를 같은 성과 숫자로 합쳐 보고하지 않는다.
5. 자체 후보로 목표 근거가 계속 없을 때 E1을 별도 설계 판단한다. 어느 경우든
   첫 canonical을 임의 K witness로 바꾸지 않는다. 실제 GUI first paint로 최종 확인한다.

각 축은 같은 binary/행렬/order/heap/cache/seed에서 직렬 ABBA 및 역순 반복으로 비교한다.
고정 worker 수 실험과 적응 worker 정책 실험을 구분한다. 1/2/4 및 host n−1/n,
모든 논리 프로세서 옵션의 controller/compute 예산을 명시하고 내부 CP-SAT threads와
외부 worker를 곱해 oversubscribe하지 않는다. 모바일 추가 실측 중단 요청은 유지한다.

다양성은 P7 좌/우·큐 부분집합·다른 kicks·강제해·dominated/equal·분리 성분·많은 동률·
UNSAT·큰 요청·word boundary·cancel/stale receipt를 포함한다. 단일 P7 속도 개선을 모든
입력의 성능 향상이라고 표현하지 않는다. 실패·timeout·미계측도 보고서에 남긴다.

계측은 feasible witness, bound progression, final negative closure, canonical 각 질의,
first paint를 분리하고 총 nodes/LP·MP work, 준비 횟수, longest cube, 버려진 duplicate
work, 유효 steal, 메모리 peak, critical-path idle을 기록한다. active 표시를 CPU 사용률로
대체하지 않는다. 충분한 반복 전에는 p90/통계적 유의성을 주장하지 않는다.

즉시 중단 조건: 원본 canonical 불일치, tie 손실, float-only prune, 잘못된 scope의
학습/receipt 재사용, known-answer hint, overflow 또는 메모리 미계상, missing/cancelled
receipt→UNSAT 변환. 성능 채택은 이 불변 조건을 모두 통과한 유효 표본에만 적용한다.

GUI 최초 canonical 3초 이내 1회 성공이라는 사용자 목표는 그대로 두되, 모든 표본과
분산도 함께 기록한다. 목표 미달이면 미달로 보고한다. 실험은 local-only이고 서버가
필요하면 **4195 하나만**, 기존 45분 lease 또는 소유 실험 종료 시 해당 프로세스만
정리한다. 4194/8790 유지, production timeout 재도입 금지, 새 CI 결과 무단 대기 금지.

## 8. 소스 지도와 이어서 읽을 기존 기록

- `exact_minimum_cover_portfolios.rs`: `ExactMinimumCoverPortfolioPreparationSession`,
  `make_parallel_oracle`, `PendingAtMostOracle::try_new`, `ExactLexQuery`.
- `exact_at_most_parallel.rs`: Arc query, exhaustive partition, `ExactAtMostShardSession`;
  `exact_at_most_assistance.rs`: unchanged parent와 child-cube racing.
- `exact_minimum_cover.rs`: dense reduction, fixed-point kernel, root/MP bound,
  excluded_rows 조건의 memo, residual dominated pivot 및 weighted-gain 분기.
- `exact_dual_lower_bound.rs`: proposal-only warm seed와 checked integer 재인증.
- `DistributedWasmJobRunner.ts`, `ClearraVerifierPool.ts`, `VerifierTransportProfile.ts`,
  `DurableDelegationJournal.ts`: wave/admission, ready subset, timer와 journal 소유권.
- `WasmJobRunner.ts`, `clearra-wasm-abi/src/lib.rs`: profiling serial-start 충돌.
- 기존 결과: `qnia-minimum-variance-ab-2026-09-08.md`,
  `minimum-browser-hotfix-ab-2026-09-08.md`, `minimum-canonical-interval-ab-2026-09-08.md`,
  `qnia-cpsat-minimum-cover-comparison-2026-09-06.md`,
  `minimum-cloud-and-legal-board-evaluation-2026-09-07.md`.
- [Qnia의 primary 모델과 exact 결과 검증][Q1], [기본 파라미터와 Worker 소유권][Q2],
  [고정 버전·런타임·license 안내][Q3].

[O1]: https://github.com/google/or-tools/blob/551ad10d94835c99e5e1e684500d3db398c0e345/ortools/sat/cp_model_search.cc#L986
[O2]: https://github.com/google/or-tools/blob/551ad10d94835c99e5e1e684500d3db398c0e345/ortools/sat/cp_model_solver.cc#L1030
[O3]: https://github.com/google/or-tools/blob/551ad10d94835c99e5e1e684500d3db398c0e345/ortools/sat/subsolver.cc#L195
[O4]: https://github.com/google/or-tools/blob/551ad10d94835c99e5e1e684500d3db398c0e345/ortools/sat/synchronization.cc#L1360
[Q1]: https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/src/ortools-primary-worker.mjs
[Q2]: https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/src/ortools-min-cover.mjs
[Q3]: https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/ORTOOLS_INTEGRATION_AND_LICENSE.md
[H1]: https://github.com/daejunnom/Clearra/blob/965d99470f15cf9759c047d3fb9ca10f894f3eeb/docs/v0.8.0-v0.9.0-implementation-release-plan.md

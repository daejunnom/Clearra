# v0.8.0 이후 P2 최소 해법 알고리즘 핫픽스 A/B

## 상태와 범위

- 배포 기준선: `495af625393d7d8a723c48b14977a87b96d68e4f`.
- 준비 브랜치: `codex/v0.8.0-hotfix-minimum-algorithm-ab`.
- Qnia 비교 기준: `03b637730c5b541f4f2934be613498fbe65327fd`.
  이번 조사에서도 이전 조사와 같은 revision임을 확인했다. 이미 확인한
  입력 차이, kernel parity, WASM RNG 수정, warm-start A/B는 다시 수행하지 않았다.
- 이 커밋은 **알고리즘 비교와 실행 가능한 후속 A/B의 계약 준비**다.
  새로운 제품 solver, 외부 의존성, 성능 향상 또는 3초 달성을 의미하지 않는다.
- 제품 v0.8.0에는 이번 CI/배포 오류 수정만 들어간다. 본 브랜치는 main으로
  병합하지 않는다. P2 핫픽스 승격 시 제품 변경만 별도로 검토한다.
- 외부 비교 실행기, 원본 소스, 행렬, solver 바이너리와 원시 계측은 `_local/`
  안에만 둔다. CI, Docker, Pages, CLI 패키지, 배포 승인 근거에 넣지 않는다.
  브랜치에는 이 기록과 비실행 JSON 실험 명세만 보존한다.

현재 배포용 canonical run `34082105759`, Pages queue `34082119851`, 과거
Discord 실행의 보호된 recovery `34082104334`는 제출만 했다. 사용자 요청에
따라 이 새 실행들의 결과를 기다리거나 조회하지 않았다. 이 문서는 그 성공을
전제하지 않으며 핫픽스의 실제 승격은 v0.8.0 배포 확인 이후다.

## 반드시 유지할 제품 의미

1. 원래 후보 ID 순서에서 **정확한 첫 canonical 최소 집합**을 먼저 반환한다.
   최소 개수 증명만 끝난 임의 집합을 첫 canonical 집합이라고 표시하지 않는다.
2. 이후 동률 집합은 사용자 요청 때 lazy하게 진행한다. 첫 결과 전 전체 개수
   계산, 숨은 다음 페이지 탐색, 모든 동률 집합 생성은 하지 않는다.
3. 첫 화면에는 집합의 기존 필드를 사용한다. CTK3는 렌더/복사 경계에서 생성한다.
   렌더 100개 단위와 현재 집합 전체 복사의 의미를 혼동하지 않는다.
4. CLI core가 의미와 증명 상태를 소유하고 GUI/Discord는 어댑터다. CountAll,
   minimum, score-minimum, Build 집계의 별도 목적 함수를 섞지 않는다.
5. Jstris 180, `ctk3_w0kCQBjwwAMPPAD37g`, P7, 4L, hold가 주 성능 fixture다.
   알려진 최소 25는 **출력 사후 조건**이지 입력 bound, hint, 캐시 답이 아니다.
6. GUI 최초 집합 3초 이내 1회 성공은 기존 P2 목표로 유지한다. 최소값만 보고하지
   않고 모든 유효 표본과 중앙값/범위를 함께 기록한다. 목표 미달은 명시한다.
   GUI/CLI 제품에 fixture용 120초 제한을 다시 넣지 않는다.
7. 오류/취소/메모리 거부/누락 receipt는 UNSAT 또는 최소 개수 증명이 아니다.
   실제 source/matrix/query/generation이 일치하는 완결 증거만 수용한다.

## 이전 실측을 어떻게 읽을 것인가

아래는 기존 연구 기록의 **과거 계측**이다. `495af62`나 새 실험 후보의 계측으로
재표기하지 않는다. 다른 clock, binary, 행렬, worker 수의 값을 직접 비율로
계산하지 않는다.

| 이전 근거 | 관측 | 해석 범위 |
| --- | --- | --- |
| Qnia 공개 GUI, Primary Auto / Quality Fast | 4,265 / 4,185 ms | feature timer; 순수 CP-SAT 시간이나 Clearra 첫 canonical 증명이 아님 |
| 동일하게 고정한 left P7 행렬의 로컬 Qnia CP-SAT native wall time | 6,075 / 8,017 / 9,853 ms | kernel 1,389 cases / 158 candidates / 15,128 incidences, K=25; 별도 환경 |
| mirrored P7 로컬 CP-SAT | 9,399 / 8,114 / 4,916 ms | 1,385 / 158 / 15,104; 좌우 입력을 동일 행렬로 취급할 수 없음 |
| first-I / first-S 720개 큐 부분집합 | 각각 218~243 / 171~238 ms | K=11 / K=10; P7 5,040개와 크기뿐 아니라 제약 구조가 다름 |
| Clearra 4-worker residual warm seed A/B | proof 10,695→9,129; canonical 14,103→12,903 ms | 합계 24,798→22,033 ms. 이미 기준선에 반영된 개선 |
| Clearra 2-worker residual warm seed A/B | proof 10,962→8,922; canonical 13,351→11,594 ms | 합계 24,314→20,517 ms. 전체 GUI timer와 구분 |

따라서 canonical의 추가 의미만으로 모든 차이를 설명할 수 없다. **K 증명과
첫 canonical 확정 모두 알고리즘 개선 대상**이다. ILC 후보 생성이나 worker
상수 조정만으로 이 차이가 해결된다고 가정하지 않는다.

## 코드 대조: 이미 있는 것과 새로 비교할 것

행렬 표기: Clearra row는 후보 해법, Qnia의 coverage case는 커버할 큐다.
두 소스의 row라는 단어를 그대로 비교하지 않고 후보/제약/incident 수를 구분한다.

| 축 | Qnia의 현재 경로 | Clearra 기준선 / 핫픽스 비교 |
| --- | --- | --- |
| 후보 생성 | 자체 PC/WASM cover matrix | ILC Geometry + BuildUp/검증 유지. 각자 생성한 행렬 비교와 동일 행렬 solver 비교를 분리 |
| 의미 정규화 | numeric key 및 coverage case 준비 | 규칙, hold, 큐 순서, canonical ID, source completeness를 먼저 binding |
| primary presolve | singleton 일괄 강제, 중복/포함 case 제거, dominated candidate 제거를 고정점까지 반복 | 기존 kernel parity 근거 재사용. 순서 변경이 아닌 삭제 증거/원본 복원 가능성을 비교 |
| solver 선택 | kernel 뒤 cases≥200, candidates≥112, entries≥2200이면 Auto CP-SAT; 작으면 Rust | Clearra의 기존 exact 경로 유지. threshold를 무근거 복사하지 않고 kernel 구조별 crossover를 측정 |
| CP-SAT 구성 | 2 workers, `max_lp`, seed=1, zero-half cuts=false, SAT inprocessing=false | LP/PB propagation, conflict learning, reduced-cost reasoning을 분리해 독립 구현 후보로 평가 |
| HiGHS 경로 | 명시 선택 또는 환경 정책 fallback의 MIP | CP-SAT와 동일 raw/kernel 입력에 대한 참고 backend. 런타임/라이선스 포함 비용은 별도 |
| 목적 함수 | 후보마다 Boolean, 각 큐를 ≥1회 커버, 선택 개수 최소 | 동일 set-cover 목적. OPTIMAL과 bound 일치 및 원본 커버 재검증을 함께 요구 |
| 상한 | backend incumbent / 후속 quality seed | 기존 greedy, repair, replay-validated witness 유지. UB 발견과 K-1 UNSAT 증명 시간을 분리 |
| 하한 | `max_lp` 중심의 CP-SAT/MIP machinery | Clearra Mirror-Prox proposal + checked integer dual certificate가 이미 존재. 더 강한 완화/절단과 전파를 평가 |
| conditional pruning | CP-SAT 정수/SAT 추론, LP 기반 추론 가능 | 기존 certified conditional row pruning을 대체/보강하되 float만으로 prune 금지 |
| 충돌 학습 | CP-SAT 설명 기반 SAT/정수 추론 | branch-local memo와 다른, 가정 조건을 포함한 재사용 가능한 nogood/학습 절 비교가 핵심 |
| 분해 | backend presolve/search 내부 처리 | 실제 독립 incidence component가 있을 때만 K 합산. 거의 분리된 separator는 별도 비용 평가 |
| 대칭 | primary cardinality 감소에 허용되는 삭제 | exact 첫 집합에는 원본 dominated/equal ID도 필요. proof-only quotient의 삭제를 canonical로 전파 금지 |
| K 이후 집합 선택 | Quality Fast가 기본; exact human-quality 경로도 별도 존재 | Clearra는 original-ID lexicographic self-reduction. Fast와 동일 의미라고 주장하지 않음 |
| prefix 처리 | Qnia 결과를 lex proof로 사용할 수 없음 | 관련된 fixed-K prefix 질의 간 가정/학습 상태 재사용을 B2로 평가 |
| bounded continuation | 외부 solver의 solve 생명주기 | Clearra는 이미 같은 in-memory 질의의 DFS/memo/incumbent를 보존. 매 slice 재시작 버그라고 다시 고치지 않음 |
| 병렬 탐색 | CP-SAT 내부 subsolver 협업; 개수만으로 Clearra worker와 비교 불가 | exhaustive shard/work stealing + controller의 책임 유지. bound/witness/학습 정보 공유 효과를 별도 측정 |
| 종료 | CP-SAT OPTIMAL+bound 및 original coverage 검증 | positive replay, complete negative receipt, stale/duplicate/cancel 거부 유지 |
| 후속 동률 | Qnia Fast의 첫 결과 timer에 포함되지 않음 | 최초 페이지와 lazy 다음 페이지를 각각 측정. 전체 동률 열거는 첫 결과 비용에서 제외 |
| 메모리/전송 | module/model build, worker load/cleanup 등 | immutable matrix 전달, peak heap, clone/merge, final projection 각각 별도 clock |
| kick별 입력 | 각 source의 실제 reachable board 집합에 의존 | Jstris와 SRS-X/SRS+를 별도 fixture로 binding. mirror/rotation 불변성 추정 금지 |

### 비교를 오염시키지 않을 기존 수정

- residual warm-start는 이미 켜져 있다. cached-pivot의 과거 별도 브랜치는
  default-off 보조 대조군일 뿐 이번 주 알고리즘 후보로 재포장하지 않는다.
- SRS-X/tetrio.js, WASM RNG 폭 차이, 공개 ID/CTK lazy projection, Build evidence
  의미, 기존 n-1/n worker 정책은 이 문서 때문에 다시 변경하지 않는다.
- code comment 중 residual warm seed가 default-off라고 적힌 과거 문구와
  `exact_dual_lower_bound.rs`의 실제 default-true를 구분한다. 동작은 후자를 따른다.

## 알고리즘 후보와 우선순위

### B1: K 증명의 정수 전파/학습과 더 강한 하한

우선 coverage OR 제약과 `sum x <= k`를 같은 reversible trail에서 전파한다.
충돌의 이유를 추적하고 decision/assumption 조건을 포함한 nogood를 남긴다.
단순 residual-state memo와 달리 관련 branch에 재사용 가능한 지식을 만드는
것이 목적이다. 전체 CP-SAT를 복제했다고 부르지 않는다.

LP/MIP 아이디어는 독립적인 B1b로 나눈다. 완화에서 찾은 dual 또는 절단 후보는
원래 정수 제약에서 다시 증명된 경우에만 pruning authority를 갖는다. 예를 들어
서로 다른 3개 coverage 제약을 합치고 올림하면
`sum_j ceil(d_j / 2) * x_j >= 2`라는 유효 절단을 얻는다. 세 제약을 모두
커버하는 후보의 계수는 **2**다. 이를 1로 평탄화하면 유효 해를 잘못 버린다.
Qnia 기본 CP-SAT 설정은 zero-half cuts를 꺼 두므로 이 후보를 Qnia 4초의 원인으로
단정하지 않는다. 절단 생성/검증 비용이 node 감소보다 큰 경우 제외한다.

### B2: 원본 ID의 fixed-K incremental canonical solver

K 증명 이후 immutable 원본 행렬을 유지하고 prefix 포함/제외를 assumptions로
바꾼다. 같은 K/행렬에서만 조건부 학습 절을 재사용한다. prefix-local selector
변수, K 상한, generation이 바뀌면 해당 범위의 학습/receipt는 폐기한다.
포함 가정을 우선 검사하는 lex self-reduction은 유지한다. 이전 질의의 UNSAT를
새 가정의 UNSAT로 무조건 재사용하거나 임의 최소 witness를 그대로 공개하지 않는다.

현재 질의 내 DFS continuation은 이미 구현되어 있으므로 B2는 **질의 사이의
추론 재사용** 실험이다. first result 직후 solver를 quiescent로 두며 next 요청이
들어왔을 때만 successor를 시작한다. 원본 ID의 전체 동률 집합은 그대로 보존한다.

### B3: 가역 presolve / 실제 독립 component 분해

component별 optimum 합산과 forced candidate 복원은 분해 증거가 있을 때만 한다.
dominance quotient는 K 증명 전용이고 canonical solver에는 원본 행렬을 전달한다.
예: rows `{a}`, `{a,b}`, `{b,c}`, `{c}`에서 K=2, 첫 집합은 `[0,2]`다.
row 0을 dominated라고 삭제하면 K는 유지해도 첫 집합 의미는 깨진다.

구조적으로 연결된 P7 kernel에서 component가 하나뿐이면 B3의 이득은 0이라고
기록하고 중단한다. 법적 보드 index나 대칭을 무리하게 분해 증거로 사용하지 않는다.

### B4: 알고리즘 협업의 병렬화 — 이차 축

동일 query에 묶인 검증된 incumbent, certified bound, 조건부 학습 절만 공유한다.
shard coverage와 complete-negative 계약은 그대로다. controller가 긴 solver
작업에 막히지 않도록 하고 준비된 worker가 실제 처리한 node/CPU를 계측한다.
1/2/4/n-1/n compute workers를 모두 기록하되 controller 수는 별도다. worker 수의
증가나 UI의 active 표시만으로 알고리즘 개선을 주장하지 않는다.

## SRP 경계와 승격 순서

제안 인터페이스는 구현 완료 API가 아니다.

- `MinimumModel`: original candidate/queue identity와 immutable coverage.
- `CardinalityKernel`: proof-only reductions와 원본 복원 증거.
- `BoundCertificate`: integer checked dual/cut authority. float proposer와 분리.
- `CardinalityProof`: K와 원본 witness, complete negative evidence.
- `FixedKAssumptionSession`: 원본 행렬에서 prefix 판정/가정 범위의 학습 소유.
- `CanonicalEnumerator`: exact lex 순서, first-only handoff, lazy successor 소유.
- `Scheduler`: bounded work/steal/cancel/memory; 제품 집합 의미는 결정하지 않음.
- `DiagnosticAdapter`: clock/metric 기록만. fixture의 알려진 답이나 외부 solver를
  제품의 hint/권한으로 전달하지 않음.

순서: B1과 B2를 독립 비교 → 이득이 증명된 조합만 B1+B2 → 필요할 때 B3/B4.
전체 CP-SAT vendoring은 독립 구현 후보가 목표를 달성하지 못했을 때 별도 검토한다.
그 경우 Apache-2.0/NOTICE, 포트와 transitive dependency 고지, 번들 크기,
JSPI/SAB/COOP-COEP와 native/WASM parity를 다시 감사해야 한다.

## A/B 실행 계약

기계 판독 명세는 같은 이름의 `.json` 파일이다. 지금은 모든 새 알고리즘 후보가
`planned`이고 제품 연결은 없다. 기존 로컬 실행기는 root checkout의
`_local/research/benchmark-qnia-cpsat.mjs`, `qnia-pure-proof.mjs`,
`compare-qnia-highs-cpsat-20260907.mjs`를 재사용한다. raw matrix는 ignored 상태로
유지하고 정확한 source/hash/ID 바인딩 검증 뒤에만 가져온다.

1. **동일 문제 solver 비교**: 같은 original matrix/순서로 kernelize 전후 시간을
   나눈다. 기존 Jstris matrix SHA는
   `63de33e1d86077c179a38f6311df893ba9abcc13c9b18fa433384f5961eeee91`.
   producer source와 candidate/queue bindings가 없는 matrix는 거부한다.
2. **제품 전체 비교**: 같은 input/rule/runtime로 Geometry, BuildUp/검증, matrix,
   proof, canonical, projection, GUI first paint를 별도로 기록한다.
   동일 행렬 solver-only 실험을 GUI 3초 성공으로 해석하지 않는다.
3. 측정쌍당 같은 host/browser/WASM identity, workers, heap 정책, seed, cache 상태를
   묶고 워밍업 후 `A B B A B A A B`로 측정한다. 실험을 동시에 돌리지 않는다.
   별도 환경에서 얻은 Qnia GUI 관측은 참고열로만 둔다.
4. 불완전/실패/취소 표본도 기록한다. null clock을 0으로 채우거나 timeout을
   성공 표본에서 조용히 제거하지 않는다. deadline은 실험 fixture에만 적용한다.
5. P7 좌/우, first-I/first-S, 작은 강제 해, duplicate/dominance, 서로소 component,
   동률이 많은 경우, UNSAT/취소/메모리 및 실제 더 큰 요청을 포함한다.
   mirrored field와 큐 부분집합은 실제 행렬을 고정하기 전엔 자동 추정하지 않는다.
6. K proof 비교에는 Qnia의 임의 optimal witness를 허용하되 원본을 재검증한다.
   canonical 비교에는 동일 원본 first-set hash와 exact prefix 증거를 별도로 요구한다.
   score objective/attack 등을 섞은 set은 비교에서 제외한다.
7. 실험용 서버가 필요한 단계에서만 4195를 사용한다. 기존 소유자가 있으면
   거부하고 4194/8790에 fallback하지 않는다. PID/lease를 기록하고 30분 만료 또는
   소유 실험 종료 때 해당 서버만 정리한다. 이번 준비에서는 포트를 열지 않았다.
8. 이 호스트의 로컬 native 실행 금지 정책은 유지한다. native 비교는 이후
   명시적으로 허용된 실행 환경에서만 한다. 새 CI 결과도 이번 턴에서 조회하지 않는다.

## 준비 단계의 로컬 논리 검증

본 worktree의 ignored `_local/research/minimum-hotfix-logic.mjs`와 대응 test는
독립 작성한 작은 논리 sandbox다. production solver 또는 CP-SAT 재구현이 아니다.
odd-row 정수 절단의 유효성, dominated row가 canonical을 바꾸는 반례, fixed-K
assumption을 포함한 학습 절의 조건, unknown을 증거로 바꾸지 않는 경계를 검사한다.
작은 exhaustive model test를 실제 P7 성능 A/B나 배포 gate 성공으로 쓰지 않는다.
샘플 실행/증거는 local-only이며 브랜치에는 결과의 범위만 기록한다.

준비 시 `node --test _local/research/minimum-hotfix-logic.test.mjs`를 실행해
**9/9 통과**했다. 80개 작은 결정적 생성 모델의 모든 K에서 learning off/on과
완전 열거를 대조했고, 생성된 학습 절도 원본의 모든 유효 해를 보존하는지 검사했다.
별도로 40개 작은 모델의 정수 절단을 확인했다. scope/cancel/canonical 반례와
실험 명세 검사도 포함한다. 외부 solver 실행, P7 실제 성능 계측, 로컬 native 실행,
WASM 교체, GUI 세션/서버 생성, CI dispatch는 이 논리 검증에 포함되지 않았다.

로컬 sandbox 재확인용 SHA256:

- `minimum-hotfix-logic.mjs`:
  `195926f580d49274fce65f2fb07dfdd820e31631823242e88ee9bf9238908452`
- `minimum-hotfix-logic.test.mjs`:
  `bdc4c427a47fd25f69213078589fbd31446b0974d19577cc975afd3a7a24bdd1`

두 파일은 `git check-ignore`로 배제됨을 확인했다. 새 A/B 명세의 B1~B4를 이미
제품에 구현했거나 실측한 것으로 해석하지 않는다. 다음 핫픽스 작업에서 후보
구현 후 이 명세에 따라 실제 동등 입력의 A/B를 수행해야 한다.

## 근거와 후속 담당 위치

- 기존 실측과 결정: `qnia-cpsat-minimum-cover-comparison-2026-09-06.md`,
  `minimum-cloud-and-legal-board-evaluation-2026-09-07.md`.
- Clearra: `crates/clearra-coverage/src/cover/exact_minimum_cover.rs`,
  `exact_dual_lower_bound.rs`, `exact_at_most_parallel.rs`,
  `exact_at_most_assistance.rs`, `exact_minimum_cover_portfolios.rs`.
- [Qnia backend adapter](https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/src/ortools-min-cover.mjs),
  [primary worker/model](https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/src/ortools-primary-worker.mjs),
  [presolve/HiGHS](https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/src/highs-cardinality.mjs),
  [adaptive quality path](https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/src/min-cover-adaptive.mjs),
  [rounded-cut experiment](https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/src/min-cover-rounded-cuts.mjs).
- [OR-Tools solver architecture](https://github.com/google/or-tools/blob/stable/ortools/sat/README.md):
  SAT/정수 전파, LP relaxation, dual-ray explanation, reduced-cost fixing 구성을 참고한다.
  [CP-SAT status semantics](https://developers.google.com/optimization/cp/cp_solver):
  FEASIBLE/UNKNOWN을 OPTIMAL/INFEASIBLE로 취급하지 않는다.
- [Qnia integration/license notes](https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/ORTOOLS_INTEGRATION_AND_LICENSE.md).
  이번에는 외부 구현을 제품에 복사하지 않았다.

# 배치 조건 그래프·큐 확률 연결과 빈 필드 P7P4 검증

작성일: 2026-11-20 KST. 상태: 프로토타입 구현, 관련 정확성 시험, 전체 P7P4 동일 바이너리 비교 완료.

이 문서는 [앞선 배치 의존성 초안](placement-dependency-prototype-2026-11-20.md)을 잇는 기술 검토다. 코드에서 확인한 사실, 완료된 시험의 결과, 아직 측정하지 않은 개선 가설을 구별한다. 실행 원문은 복사하지 않고 검토한 수치와 구현 근거만 기록한다.

## 1. 이번 작업의 결론과 범위

Clearra에는 이미 정확한 배치 순서 그래프를 미노 순서 언어로 바꾸고, 이를 bag·hold 공급 상태와 결합하여 성공 큐의 합집합과 확률을 계산하는 경로가 있었다. 이번 작업은 별도의 확률 엔진을 만드는 대신 그 기존 연결에서 중복 순회를 줄이고, 배치 조건 재사용의 실행 비용을 낮춘다.

구현한 변화는 다음과 같다.

- CountUnique의 기존 물리 BuildUp 경로에서 live node 판정과 suffix 언어 canonicalization을 한 역방향 순회로 합친다.
- 수락 terminal이 없는 graph는 언어가 비어 있음을 이미 알 수 있으므로 언어 후처리를 통째로 생략한다.
- 관측 relation을 piece·scope별 bucket으로 찾아 전체 목록 순회 비용을 줄인다. 실제 일치 판정은 기존의 정확한 조건 비교를 유지한다.
- 증명 보관 예산의 남은 용량이 다음 증명보다 작아진 경우, 해당 catalog·profile scope에서 추가 증명 생성을 중단한다. 기존 증명 조회와 정확한 primitive 탐색은 계속한다.
- 실험 모드 환경 변수는 process 시작 시 한 번 읽고, 시험의 scoped override는 별도로 유지한다.

이 경로는 `local-search-ab` feature 또는 시험 빌드에서만 존재한다. 호스트 A/B는 `CLEARRA_PLACEMENT_DEPENDENCY=0`과 `1`을 별도 process에서 선택한다. 일반 제품 빌드의 동작과 worker 선택 정책을 바꾸지 않는다. 구현 근거는 [buildup.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs), [piece_order_language.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/piece_order_language.rs), [placement_dependency.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/placement_dependency.rs), [reachability_dependency_cache.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_dependency_cache.rs)다.

최종 동일 바이너리 비교는 기준 평균 128.350초, 실험 평균 112.108초로 12.655%의 시간 감소를 관측했다. 두 모드를 각각 두 번 실행했고, 전체 해 ID와 global 성공 큐 비트셋은 네 실행 모두 일치했다. 이 결과는 아래 명시한 Rust core·Jstris 180·9 worker·해별 확률 Omit 조건의 관측값이다. 모든 해별 큐 목록을 동시에 제공하는 비용이나 다른 backend의 성능은 이 측정으로 확정하지 않는다.

## 2. 수학적 표현: 선택, 실행, 줄 지우기

### 2.1 X, E, D를 구분한다

| 기호 | 의미 | 정확히 알려지는 시점 |
| --- | --- | --- |
| X | 타일링에서 선택한 canonical placement 집합 | 타일링 부분 선택마다 갱신 |
| E | 고정된 candidate에서 실제 실행한 operation 집합 | BuildUp의 각 상태 |
| D(E) | 실행 E로 이미 삭제된 원래 행 집합 | 고정된 candidate의 contributor 조건을 평가한 뒤 |
| A(p) | placement p가 ILC로 실현 가능한 정확한 D의 집합 | GeometryCatalog 컴파일 직후 |

canonical placement는 종류와 원래 좌표계의 셀 집합으로 구분한다. 같은 종류의 미노라도 셀 집합이 다르면 다른 placement이며, candidate에서 서로 다른 operation bit를 갖는다. `X에 포함된다`는 사실은 `E에서 실행되었다`는 사실이 아니다.

고정된 candidate C에서 원래 행 r을 채우는 operation 집합을 P(r)라고 하면, 초기 보드의 셀을 상수로 처리하여 실제로 완성 가능한 행에 대해서만 다음 관계를 사용한다.

```text
Clear(r, E) = P(r)의 모든 operation이 E에 포함됨
D(E) = { r | Clear(r, E) }
B(E) = Compact(initial_board OR executed_original_cells(E), D(E))
```

동일한 contributor ID 집합을 가진 행들은 하나의 AND 조건과 복수 행 마스크로 묶인다. 한 operation이 두 행을 동시에 완성하면 두 원래 행의 삭제가 같은 사건으로 발생한다. 일부만 채워지는 목표 행은 삭제 조건에 포함하지 않는다. [placement_conditions.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs)의 `RowConditions`와 [buildup.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs)의 `CandidateProjection`이 이 의미를 구현한다.

고정된 candidate에서는 E가 D와 compacted board를 결정하므로, BuildOrderGraph는 E를 정확한 상태로 사용할 수 있다. 이는 hash가 비슷한 보드를 합치는 근사가 아니다. 다른 predecessor가 같은 E로 들어오는 모든 유효 edge를 보존한다.

### 2.2 ILC domain은 이동 도달성 증명이 아니다

`PlacementConditionCatalog::compile`은 타일링 전에 각 placement p와 각 D에 대해 기존 `catalog.instantiations(p, D)`가 하나 이상 존재하는지 조사한다. 높이 1–6의 Board64 범위에서는 최대 64개 D를 정확한 비트셋 A(p)로 표현할 수 있다. 같은 비트셋은 공유하지만 placement의 정체성은 합치지 않는다.

이 domain은 pending placement의 셀이 이미 삭제된 경우를 포함해 ILC 실현 가능성을 판정한다. grounding, collision, spawn에서 목표까지의 이동·회전 도달성은 별도의 조건이다. 반대로 이미 실행된 placement의 살아남은 셀은 이후 collision이나 kick 선택에 영향을 줄 수 있으므로, 지금 pending ILC가 불가능하다는 이유로 영향 관계에서 제거하면 안 된다.

현재 A(p)의 production 소비 지점은 [catalog.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs)의 `instantiations`다. 정확한 D를 이미 알고 있는 BuildUp 질의에서 domain이 false이면 빈 iterator를 반환한다. APDP의 부분 X 선택에서 이 domain을 전파하는 연결은 아직 없다.

## 3. 그래프에서 큐와 확률을 계산하는 기존 연결

### 3.1 정확한 배치 그래프에서 미노 순서 언어로

BuildOrderGraph의 edge에는 concrete operation, kind, realization 정보가 있다. edge를 추가하기 전에 ILC instantiation, grounding, 물리 배치·줄 지우기, 선택한 reachability 정책의 검증을 수행한다. 수락 node는 모든 operation을 실행한 상태에서 completion 조건까지 만족하는 node다.

상태 E 이후 가능한 미노 순서들의 집합을 L(E)라고 하면 다음과 같이 계산한다.

```text
L(accepting terminal) = { empty word }
L(E) = UNION over valid edges (E --kind--> E') of kind · L(E')
L(dead state) = empty set
```

`PieceOrderLanguageCache`는 bottom-up으로 같은 suffix 구조를 intern한다. 이것은 모든 concrete 배치를 다시 실행하는 연산이 아니라 이미 검증된 graph를 읽는 집합 연산이다. 동일한 kind와 child에 이르는 중복 edge는 Boolean 언어에서 합쳐도 된다. 같은 kind로 서로 다른 child에 가는 edge는 각각의 suffix를 합집합으로 남겨야 한다. suffix가 일부 겹친다고 두 분기 중 하나를 선택해서는 안 된다.

기존 canonicalization은 이미 이러한 공유를 수행했다. 이번 fusion은 공유하는 언어의 의미를 바꾸지 않는다. 언어 구조가 같은지를 확인할 때에도 hash만으로 동등성을 인정하지 않고 실제 node·edge tuple을 비교한다. [piece_order_language.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/piece_order_language.rs)를 참조한다.

### 3.2 언어와 bag·hold 상태의 product

`StandardBagCoverage`의 memo key는 다음 정확한 tuple이다.

```text
(piece-language node, source depth, exact bag remainder, hold piece)
```

현재 piece를 바로 사용하는 행동, held piece와 바꾸는 행동, 빈 hold에 현재 piece를 보관하고 다음 draw를 사용하는 행동을 기존 공급 규칙대로 처리한다. 성공 분기의 결과는 원래 공급 큐를 나타내는 기호적 집합으로 union한다. 여러 실행 순서나 hold 행동이 같은 원래 큐를 성공시켜도 그 큐의 확률 질량은 한 번만 포함한다. projected lookahead와 initial hold cursor도 기존 공급 의미를 유지한다. 구현은 [standard_bag_coverage.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage.rs)에 있다.

해법 C가 성공시키는 원래 큐 집합을 Q(C), 전체 원래 공급 공간을 Omega라고 하면 다음 관계를 사용한다.

```text
Q(C) = { q in Omega | q와 hold 규칙으로 L(C)의 어떤 순서를 실행할 수 있음 }
Q(all solutions) = UNION over C of Q(C)
P(C) = SUM over q in Q(C) of source_probability(q)
```

균등한 공급 공간일 때에만 마지막 식이 `|Q(C)| / |Omega|`가 된다. 전체 성공 확률 역시 해별 확률의 단순 합이 아니라 `Q(all solutions)`의 질량이다. 그래프 경로 수나 위상 정렬 수를 공급 분모로 나누는 방식은 중복 큐 때문에 일반적으로 틀린다.

이번 P7P4 fixture의 원래 공급은 첫 7-bag 전체와 다음 bag의 순서 있는 4개 prefix다. 따라서 원래 큐 수는 `7! × (7 × 6 × 5 × 4) = 4,233,600`이다. 빈 10×4 필드는 10개 tetromino로 채우며, 공급 window는 hold·lookahead 의미를 포함한 11개다. 이 분모는 placement graph 압축이나 relation cache hit 수로 바뀌지 않는다.

### 3.3 CountUnique, CountAll, 관측 정보의 경계

CountUnique의 공급 coverage에서는 어떤 해법이 가능한지에 대한 Boolean OR가 적절하다. 같은 종류의 서로 다른 placement나 hold 행동이 동일 큐를 성공시키더라도 큐를 중복 세지 않는다.

CountAll은 다른 목적이다. concrete operation edge와 공급 행동의 multiplicity를 보존하는 기존 `CoverageProductEvaluator`를 사용한다. 예를 들어 current I와 held I가 같은 kind여도 다른 공급 행동일 수 있다. Boolean suffix 언어만으로 이 경로 수를 대체하지 않는다. finesse·spin·scoring이 요구하는 물리 증거 역시 별도 경로를 유지한다.

queue observation이 요구하는 정보도 기존 정책대로 언어 root에서 제공한다. 이번 대형 fixture는 full-future-oracle 큐 지식 조건이다. 그 성공 집합은 미래 큐를 미리 알 수 있는 해법 존재성의 결과이며, 제한된 관측으로 작동하는 online 정책의 성공률과 같은 명제가 아니다.

### 3.4 해별 결과의 제공 범위: Include와 Omit

이번 대형 입력은 기존 기본값인 `PcSolutionProbabilityPolicy::Omit`을 유지한다. full graph 경로에서는 후보의 정확한 symbolic queue root와 covered count를 계산해 global root로 합치지만, 완료 결과에 모든 identity와 root의 대응을 보관하지 않는다. global coverage가 이미 충분하면 membership-only shortcut으로 해당 해의 존재성만 확인할 수도 있다. 따라서 Omit 실행에서 모든 해의 전체 큐 집합을 내부적으로 한 번씩 계산했다고 가정해서는 안 된다.

`Include`는 추가 작업을 한다. [result.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/result.rs)는 `solution_coverage_required`를 켜고 membership-only shortcut을 비활성화한다. 후보의 symbolic root를 `PatternBitSet`으로 전개해 identity별로 보관·병합하고, 최종 확률 보고서에서 큐 가중치를 합산한다. 이번 작은 differential 시험은 Include를 켜고 이 해별 대응까지 검증했다. 대형 결과의 `solution_probability_complete=true`는 요청하지 않은 항목의 상태이며, 실제 해별 coverage/probability materialized 수는 각각 0이다.

Include로 얻은 결과에서는 `normalized_solution_coverages()`와 `solution_probabilities()`를 normalized key로 조회하고, 동일한 공급 universe의 pattern ID를 큐로 해석할 수 있다. Omit 결과에서 나중에 선택한 해의 root를 즉시 읽는 보존 API는 현재 경로에 없다. 기존 WASM CPU API로 colored identity 필터와 Include를 지정해 새 실행을 할 수 있지만, 이는 저장된 그래프의 lazy 조회가 아니다. 같은 종류 미노 때문에 하나의 색칠 결과에 여러 canonical tiling이 대응할 수 있으므로 normalized key를 추가로 구분해야 한다.

## 4. 이번 구현이 줄이는 비용

### 4.1 Live 판정과 suffix canonicalization 결합

기존 경로는 graph 생성 후 reverse-live pass를 수행하고, live root인 경우 언어 canonicalization을 수행했다. 실험 경로는 `canonicalize_and_mark_live`가 두 결과를 같은 역방향 순회에서 만든다. child의 suffix가 비어 있으면 해당 edge는 빈 언어를 기여한다. accepting이거나 빈 suffix가 아닌 child를 하나 이상 가지는 node만 live다.

적용 조건은 다음 세 조건과 실험 활성화다.

- `BuildReachabilityMode::Existing`
- `PcCountPolicy::CountUnique`
- `piece_language_projection_only == true`

CountAll, GeometryOnly로 만든 finesse graph, operation projection을 요구하는 scoring reconstruction에는 적용하지 않는다. 캐시된 language root는 해당 workspace의 언어 arena에 속하며, 그 arena를 recycle하기 전에 소비한다. finesse가 edge를 바꿀 수 있는 mutator는 캐시된 root를 지운다.

없어진 것은 별도 live 순회와 중복 graph 판독의 일부다. physical edge 생성, 필요한 reachability, suffix 정렬·dedup·intern, bag·hold product는 남는다. 이미 있던 큐 공유 계산을 이번에 처음 만든 것으로 설명하면 안 된다.

### 4.2 수락 terminal이 없는 graph의 언어 후처리 생략

현재 builder의 모든 node는 `live=false, accepting=false`로 시작한다. accepting을 설정하는 지점은 `subset == all_placed`에서 `completion.accepts`를 평가하는 곳뿐이다. 이 결과를 `accepting_reached`에 누적한다.

정상 BFS 종료 후 `accepting_reached=false`이면 graph 어디에도 accepting terminal이 없다. 따라서 모든 suffix 언어가 비어 있고 현재의 모든 live=false가 이미 최종 답이다. 이 경우 class 배열 초기화, edge 재순회, canonicalization 호출을 생략한다. scratch 반환, graph 구성, reachability state 집계는 그대로 수행한다.

`all_placed`를 발견했다는 사실만으로 수락을 가정하지 않는다. `ClearToEmpty`는 최종 보드가 0이어야 하고, `ExactBoardAfterLineClears(expected)`는 expected와 같아야 한다. 두 completion의 실제 수락 결과를 사용한다. 수락 terminal이 하나라도 있으면 다른 dead 분기를 판정해야 하므로 기존 fused 순회를 실행한다.

`fused_language_graphs/nodes/edges`는 실제 canonicalization 호출과 입력 크기를 센다. `skipped_dead_language_graphs/nodes`는 생략된 graph와 node를 별도로 센다. 이 구별은 단순 stage 시간 이동과 실제 작업 제거를 분리하는 데 필요하다.

### 4.3 Relation bucket과 process 모드 cache

relation lookup은 piece와 target scope를 이용한 256개 bucket에서 whole-family와 정확한 target 후보를 찾는다. bucket hash 충돌은 후보를 함께 보게 할 뿐, 결과 동일성의 증거로 쓰이지 않는다. 최종 재사용에는 catalog owner, kick profile, 원래 행 frame, piece·target scope, footprint의 정확한 점유 조건이 필요하다.

catalog owner는 Weak allocation identity로 구별한다. 다른 catalog나 profile이 들어오면 relation, 정적 mask, 영향 ID 사용량, bucket, admission 상태를 초기화한다. 같은 물리 board라도 D가 다르면 frame 검사를 통과하지 못한다.

실험 환경 변수의 반복 조회·할당은 process `OnceLock`으로 제거했다. 시험은 thread-local scoped mode를 먼저 확인하므로 같은 process의 A/B 시험 격리는 유지한다. 환경 변수를 실행 도중 바꾸는 방식의 A/B는 지원하지 않고 별도 process를 사용한다.

### 4.4 저장할 수 없는 증명의 반복 생성을 중단한다

cache 한도는 relation 128개, static mask 2,048개, 영향 ID capacity 128 Ki개다. 문제는 보관된 영향 ID 수가 최대값에 정확히 도달하지 않아도 다음 증명을 저장할 수 없다는 점이었다.

완료된 빈 4L catalog fixture에서는 canonical row가 764개였다. 보드 전체 footprint의 cell incidence를 수집한 벡터는 capacity 3,056을 유지했다. dedup 후 logical ID 수가 줄어도 이미 확보한 capacity는 그대로 메모리 예산에 포함한다. 42개 relation을 보관하면 사용량은 128,352이고, 총 131,072 중 2,720이 남는다. 다음 capacity 3,056을 저장할 수 없지만 기존의 `used == limit` 조건은 false다.

이 상태에서 계속 증명을 만들면 footprint 수집·incidence 벡터 할당·sort·dedup을 반복하고 저장 단계에서 다시 버릴 수 있다. 현재 구현은 처음 capacity 예산을 넘기는 admission 실패에서 `observation_admission_closed=true`로 바꾸고, 같은 owner·profile scope의 이후 새 관측 가공을 중단한다.

이는 더 작은 후속 증명을 저장할 기회도 포기하는 보수적인 cache 정책이다. 기존 relation hit는 유지하고, miss는 기존 정확한 primitive로 처리한다. 새 false 판정이나 prune 근거를 만들지 않는다. owner 또는 profile이 바뀌면 admission을 다시 연다. 이 현상과 fallback·reset 검증은 [reachability_dependency_differential_tests.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_dependency_differential_tests.rs)에 있다.

`primitive_states` counter는 dependency observer가 결과를 기록한 primitive state만 센다. 조기 반환된 관측은 빠지므로 전체 reachability 작업량과 같지 않다. 전체 A/B의 탐색량은 `CoreExecutionResult.total_reachability_states`로 비교해야 한다.

## 5. 유지해야 하는 정확성 경계

| 조건 | 현재 처리 | 허용되지 않는 확대 해석 |
| --- | --- | --- |
| 같은 kind의 다른 placement | canonical ID와 execution bit를 구별 | kind만 같으면 contributor나 node를 합치기 |
| ILC realization 대안 | 정확한 D domain에서 OR | 최소 required-clear mask 하나로 모든 대안 대체 |
| 역방향 성공·실패 증명 | 검사한 target pose의 Boolean | 다른 pose나 전체 lock family의 부정 결과로 확대 |
| exhaustive forward 증명 | 같은 guard에서 전체 lock family 재사용 | 미완료 탐색을 완전한 부정 증명으로 사용 |
| D 변경 | 다른 row frame으로 처리 | footprint 밖 placement가 줄을 지워도 기존 증명 유지 |
| unsupported·cache miss·admission 종료 | 기존 정확 계산으로 fallback | Unknown을 false나 불가능으로 처리 |
| CountAll | concrete edge·공급 행동의 multiplicity 유지 | Boolean 언어의 경로 수로 대체 |
| hash·bucket 일치 | 정확한 key와 guard 비교를 계속함 | hash만으로 cache hit 인정 |

킥의 최대 변위만으로 독립성을 단정하지 않는다. 이동 경로, grounding, spawn/sky 진입, 먼저 검사한 kick 후보의 실패 조건도 footprint에 포함되어야 한다. footprint 바깥 변화라는 증명은 같은 row frame과 같은 결과 scope에서만 유효하다.

기존 `RealizationFeasibilityWorkspace`의 완전한 subset prepass는 `PRODUCT_COMPLETE_REALIZATION_FEASIBILITY_PREPASS_ENABLED=false`이며 Unknown을 반환한다. 이번 작업은 이를 재활성화하지 않았다. 이름에 dependency graph가 있다는 이유로 이 prepass를 켜면 BuildUp과 유사한 상태 순회를 앞에서 한 번 더 수행할 수 있다. [realization_feasibility.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/realization_feasibility.rs)를 참조한다.

## 6. APDP와 아직 연결되지 않은 부분

현재 [geometry_domain.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_domain.rs)의 `row_feasible`는 placement cell이 remaining에 포함되는지와 kind가 feasible mask에 있는지를 확인한다. APDP는 3-cell partial의 exact parent support를 사용한다. 이 API는 실행 E, 삭제 행 D, 관측 relation을 입력받지 않는다.

`apdp_row_is_static_exact`는 APDP support가 있고 모든 realization의 required-deleted mask가 0인 row에만 정적 정확성을 부여한다. 줄 지우기에 의존하는 parent를 조건부 domain으로 전파하는 일반화는 아직 없다.

특히 [geometry.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs)의 `FamilyCompiler::advance`는 `advanced_analysis_enabled && initial_board != 0`일 때만 APDP를 포함한 `DomainPropagation::compile`을 호출한다. 빈 필드에서는 `compile_minimum`을 호출한다. 따라서 빈 필드 P7P4 결과를 APDP 부분 선택 전파의 성능으로 해석하면 안 된다.

`PlacementConditionCatalog::specialize`는 현재 완성된 candidate의 row ID 목록으로 호출된다. `RowConditions::clear_delta`는 test-only이고, 선택·되돌리기 watcher나 실제 dirty update에는 연결되지 않았다. `influencing_placements`도 계산·보관하지만 production lookup은 occupancy guard를 비교하며 그 ID 목록을 소비하지 않는다.

`CandidateProjection`은 이미 E별 D·물리 보드를 generation별로 저장한다. `confirm_transition`은 성공한 전이에서 계산한 child 보드도 저장한다. 따라서 새로운 구조의 목표는 기존 보드 cache를 복제하는 것이 아니라 ILC 허용 operation과 relation 유효성의 갱신을 줄이는 것이어야 한다.

## 7. 완료된 정확성 검증

검증 담당 실행에서 관련 고유 시험 59개가 통과했다. 별도 asset을 요구하는 기존 시험 2개는 ignored 상태이며 통과 수에 포함하지 않는다. 이 수치는 관련 시험 집합의 결과이며 전체 저장소 CI나 배포 게이트 통과를 뜻하지 않는다.

### 7.1 명시적 큐 126개와 해별 mapping 교차 검증

[graph_probability_integration_tests.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/graph_probability_integration_tests.rs)의 2-piece fixture는 초기 보드 `0xff3c0`, 높이 2, 공급 길이 2다. 각 설정의 표준 bag 공간은 42개 큐다.

- hold off, initial hold 없음: 성공 큐 2/42.
- hold on, initial hold 없음: 성공 큐 2/42.
- hold on, initial hold O: 성공 큐 12/42.

각 큐를 prototype off의 fixed-sequence 문제로 독립 실행하여 총 126개를 비교했다. symbolic 결과의 해당 큐를 성공시키는 solution identity 목록과 explicit 결과의 전체 identity 목록을 대조했다. on/off의 해별 coverage, 해별 probability, global coverage와 normalized identity도 함께 확인했다. 따라서 이 fixture의 근거는 aggregate numerator만 같은 수준을 넘어서 해별 큐 대응을 검사한다.

### 7.2 Dead·live candidate가 섞인 workspace 재사용

초기 보드 `0xfc3f_0fc3_f0`, 높이 4, 4개 미노, hold off, CountUnique fixture에서 on/off 모두 normalized solution 135개와 성공 큐 173/840을 얻었다. 해별 큐 mapping과 probability도 같았다.

실험 경로는 실제 fused canonicalization 135회, dead graph 후처리 생략 321회, 생략한 node 합계 685개를 기록했다. 같은 workspace가 dead·live candidate를 모두 처리하므로 이전 graph의 language class가 생략 경로를 통해 잘못 재사용되는 문제를 함께 검사한다. 이것은 작은 정확성 fixture의 작업량이며 P7P4의 시간 절감 비율이 아니다.

### 7.3 같은 kind, 줄 지우기, scope와 예산 경계

CountAll의 current I/held I 시험은 held I가 추가되었을 때 기존 variant 수의 2배가 유지되고 fused 경로 호출이 0임을 확인했다. 언어 단위 시험은 같은 kind·같은 child 중복, 같은 kind·다른 suffix 분기, 일부 겹치는 suffix union, dead child와 이전 graph class 재사용을 검사한다.

relation 차등 시험은 row frame, profile, target scope, capacity 한계와 admission 종료 후의 기존 hit·정확 fallback·owner/profile reset을 검사한다. 앞서 기록한 764 rows, capacity 3,056, 42개 보관, 사용량 128,352, 잔여 2,720은 실제 catalog fixture의 관측값이다.

## 8. 대형 측정 계약과 증거의 범위

실행 진입점은 [placement_graph_bench.rs](../../crates/clearra-core-executor/examples/placement_graph_bench.rs)다. release 빌드의 `parallel,local-search-ab` feature를 사용하고 Rust manager의 유한 benchmark profile 아래에서 실행한다.

이것은 native host에서 `WasmCpuSearchBackend`를 직접 호출하는 Rust core benchmark다. 공개 CLI 전체, GUI/browser, C backend의 측정이 아니다. 이름에 wasm_cpu가 있어도 browser/WASM 실행 시간으로 보고하지 않는다. App/UI 초기화·렌더링 비용은 포함하지 않는다.

전체 CLI release 빌드를 시도했으나 `clearra-app`의 rustc가 SIGKILL로 종료됐다. 해당 감독 기록의 memory pressure event는 0이어서 원인을 OOM으로 확정하지 않았다. 같은 CLI 빌드를 자원 설정을 바꿔 반복하지 않고, 동일한 Rust 검색 코어만 호출하는 작은 example 진입점으로 측정했다. example의 release 빌드와 아래 대형 실행들은 정상 완료했다.

입력과 실행 계약은 다음과 같다.

- 빈 10×4 필드, 4L PC, 배치 10개, 원래 공급 P7P4 11개.
- Jstris-180 규칙, empty hold enabled, CountUnique, full-future-oracle 큐 조건.
- tablebase, exact legal-board accelerator, 기존 conditioned reachability 가속은 비활성화한 동일 비교 조건.
- worker는 Auto와 all-logical-processors 정책을 사용한다. 실제 hard limit·workers used·active workers를 결과에서 검증하며, 사용자 환경의 12 logical processor에 맞추기 위해 임의로 제한하지 않는다.
- 같은 binary SHA와 같은 입력·CPU 설정으로 mode 0과 mode 1을 별도 process에서 실행한다.
- problem compile, core search, 결과 해시 생성 시간을 구분한다.
- complete flag, resource truncation, 전체 materialized identity 수, normalized set oracle, 전체 global coverage bitset을 확인한다.

사용자가 제시한 12 logical processor·약 4분은 사용자 환경의 참고 관측값이다. 규칙·제품 실행 경로·CPU·전력 상태가 같은 측정으로 검증하지 않았으므로 이 수치와 호스트 시간을 나누어 알고리즘 개선 배수로 쓰지 않는다.

### 8.1 전체 ID와 해별 큐 mapping은 다른 검증이다

대형 runner는 노출된 전체 normalized identity 배열을 canonical CTK1 encoding과 길이 prefix로 해시하고, original-universe global coverage의 모든 word를 별도로 해시한다. materialized 수가 reported total과 같은지도 확인한다. 일부 page의 hash를 전체 해 집합의 증거로 사용하지 않는다.

전체 identity와 전체 global coverage가 같다는 것은 중요한 집합 검증이지만, 각 solution에 연결된 큐 mapping 전체가 같다는 명제까지 보장하지 않는다. 대형 runner의 비교에는 모든 `(solution identity, covered queue)` 쌍의 digest가 없다. 해별 mapping은 앞의 작은 explicit-queue fixture에서 검증한 범위로 명시한다. 출력한 큐 ordinal 표본도 전체 큐 export로 취급하지 않는다.

hash는 대형 완료 결과를 비교하기 위한 digest다. solver 내부의 memo key를 hash만으로 같다고 판정하거나 prune 권한을 부여하는 용도가 아니다.

### 8.2 초기 실행 기록과 최종 재측정의 구분

초기 binary의 baseline A1은 완료되었으며 core search는 179.623760937초였다. normalized identity 456,459개 전체와 global 성공 coverage 4,233,600개를 확인했다. 이 실행은 다음 digest를 남겼다.

- 전체 ordered CTK1 identity SHA-256: `08c477a2f71455a72778f076f9942789a9997ab3b3008fc2a3d043f02458f062`.
- 전체 global coverage words SHA-256: `0d7794467640c727b89ccd5f170fc4430228e0aba6fa9d7f8657b0e653ef3912`.
- total build-order nodes: 637,208,218.
- total reachability states: 3,283,063,695.

같은 초기 구현의 treatment B1은 약 7분 후 Ctrl-C로 중단했으며 exit 130인 미완료 실행이다. 완료 시간, 완성된 결과 해시, A/B 속도 비율의 근거로 사용하지 않고 비교 통계에서 제외한다. 이 상태로는 초기 구현이 대형 탐색을 개선했다고 결론 낼 수 없다.

이후 dead terminal skip과 admission 중단을 반영했으므로 최종 비교는 수정된 동일 binary의 baseline/treatment로 다시 수행해야 한다. 초기 A1과 수정된 treatment만 묶은 값을 최종 같은-binary A/B로 보고하지 않는다.

초기 측정 소스는 로컬 commit `5fa1d50`으로 보존했다. 초기 코드의 remote commit object는 `bf1bd63b351259cbf3a56120439a7e6209151437`이다. 이후 수정한 재측정 코드와 binary의 식별자는 다음과 같다.

- 재측정 코드 remote commit object: `fc25dc4f32d932324d30a21b4594646a1826bbd8`.
- 재측정 코드 tree: `290b7b03000ee885c3ed74adf6c33f047703fd90`.
- release binary SHA-256: `740435770290cad5aee116f0777e9c5ea18582ca3259c57ee1e1127de606bc52`.

### 8.3 최종 측정 결과

환경은 Linux, AMD EPYC 9V74, 감지된 논리 프로세서 9개, 물리 메모리 10,451,427,328 bytes, Rust 1.98.1이다. CPU 모델의 전체 core 이름과 이 실행에 할당된 CPU 수는 구분한다. `Auto + use_all_logical_processors=true`로 실제 workers used와 active workers 모두 9를 확인했다. `benchmark-search` 감독 profile과 각 실행 1,800초 제한을 사용했으며 solver 내부 candidate·memory 제한은 추가하지 않았다.

2026-11-20 08:29–08:39 KST에 **B2 → A2 → A3 → B3** 순서로 같은 release binary를 실행했다. A는 mode 0, B는 mode 1이다. 검색 실행 중 다른 빌드나 별도 검색은 수행하지 않았다. 시간은 core search wall time이며 problem compile과 증거 해시 생성은 분리했다. 메모리는 감독기가 관측한 프로세스 트리 peak이며 엔진의 추정 retained-byte 지표와 다르다.

| 실행 | 모드 | Search 초 | 감독 peak bytes | 감독 peak GiB | Total reachability states |
| --- | --- | ---: | ---: | ---: | ---: |
| A2 | 기준 | 136.123157067 | 5,007,532,032 | 4.664 | 3,278,454,820 |
| A3 | 기준 | 120.577717319 | 5,136,441,344 | 4.784 | 3,285,447,985 |
| B2 | 실험 | 112.041757571 | 5,144,088,576 | 4.791 | 3,239,726,049 |
| B3 | 실험 | 112.174322700 | 5,149,298,688 | 4.796 | 3,244,927,579 |

| 요약 | 기준 | 실험 | 변화 |
| --- | ---: | ---: | ---: |
| 평균 search | 128.350437193초 | 112.108040136초 | **12.6547% 감소** |
| 평균 시간 절감 | — | 16.242397057초 | 약 1.145배 처리 속도 |
| 평균 reachability states | 3,281,951,402.5 | 3,242,326,814 | 1.2073% 감소 |
| 타일링 후보 | 29,856,840 | 29,856,840 | 동일 |
| 누적 build-order nodes | 637,208,218 | 637,208,218 | 동일 |
| 평균 coverage product states | 156,381,493 | 156,325,136.5 | 거의 동일 |

첫 쌍의 시간 감소는 17.6909%, 반대 순서 두 번째 쌍은 6.9693%였다. 기준 실행의 편차가 있으므로 12.6547%는 이 환경에서 각각 두 번 얻은 관측 평균이다. p95, 통계적 유의성, 다른 CPU·backend·규칙에서의 동일 개선율은 주장하지 않는다. 두 번의 실험 peak 메모리는 기준보다 약간 높았으므로 메모리 절감 효과를 주장하지 않는다. 네 실행 모두 정상 종료·memory pressure event 0이었다.

Rust runner의 `--compare` 결과는 `equivalent=true`, `differences=[]`다. 네 실행 모두 다음을 만족했다.

- 전체 normalized solution count 및 실제 반환된 identity 수: **456,459**.
- 전체 normalized set oracle: `cts1:2fcbc950acdffa31`.
- 전체 ordered CTK1 identity SHA-256: `08c477a2f71455a72778f076f9942789a9997ab3b3008fc2a3d043f02458f062`.
- 전체 성공 공급 패턴: **4,233,600 / 4,233,600**, global probability **1**.
- Global coverage: **66,150 u64 words** 전체 SHA-256 `0d7794467640c727b89ccd5f170fc4430228e0aba6fa9d7f8657b0e653ef3912`.
- Count/probability complete, resource truncation 없음, 실제 parallel execution과 9 workers.

이 100%는 전체 큐를 미리 아는 조건에서 각 큐에 대해 어떤 해법이 존재한다는 뜻이다. 특정 해법 하나의 성공 확률이 100%라는 뜻이나 online 정책의 보장은 아니다.

물리 도달성 방문 상태의 평균 감소는 약 1.21%이며, 타일링 후보와 build graph node 수는 동일하다. 이번 시간 차이는 조건 조회·예산 관리·언어 후처리 변경을 합친 측정값이다. 변경별 단독 ablation이나 stage별 CPU profiling을 수행하지 않았으므로 12.65% 중 각 변경의 기여율을 분배하지 않는다. 초기 B1의 중단과 후속 개선 사이에도 두 수정을 함께 적용했으므로 admission 문제만을 지연의 유일한 원인으로 확정하지 않는다.

### 8.4 재현

다음은 이 Linux checkout에서 사용한 진입점이다. `clearra-manage`가 이미 빌드돼 있다는 전제이며 출력 경로는 저장소에 선언된 retained evidence root다.

```sh
build/cargo/default/release/clearra-manage runtime run \
  --producer cargo --profile build-test --timeout 1800 -- \
  "$HOME/.cargo/bin/cargo" build --release --locked -p clearra-core-executor \
  --features parallel,local-search-ab --example placement_graph_bench

mkdir -p _local/artifacts/placement-graph-p7p4-20261120

CLEARRA_PLACEMENT_DEPENDENCY=0 \
  build/cargo/default/release/clearra-manage runtime run \
  --producer benchmark --profile benchmark-search --timeout 1800 -- \
  build/cargo/default/release/examples/placement_graph_bench \
  > _local/artifacts/placement-graph-p7p4-20261120/repro-a.json

CLEARRA_PLACEMENT_DEPENDENCY=1 \
  build/cargo/default/release/clearra-manage runtime run \
  --producer benchmark --profile benchmark-search --timeout 1800 -- \
  build/cargo/default/release/examples/placement_graph_bench \
  > _local/artifacts/placement-graph-p7p4-20261120/repro-b.json

build/cargo/default/release/examples/placement_graph_bench --compare \
  _local/artifacts/placement-graph-p7p4-20261120/repro-a.json \
  _local/artifacts/placement-graph-p7p4-20261120/repro-b.json
```

`--small`은 실행기의 사전 확인용 1-piece fixture이며 그 미세한 시간 차이는 성능 근거로 사용하지 않는다. 원본 대형 JSON, 감독 receipt, 초기 중단 상태, 비교 JSON을 별도 결과 묶음으로 보존한다. 본 문서는 검토한 요약이며 raw 실행 로그 저장소가 아니다.

## 9. 다음 구조 개선 후보

아래 항목은 코드 연결 지점과 안전 조건을 정리한 후속 계획이다. 성능 효과는 아직 측정하지 않았다.

### 해별 확률과 큐 출력의 분리

사용자가 요청한 해별 결과를 대형 탐색에서도 효율적으로 제공하려면, scalar 확률 계산과 모든 큐 비트셋의 materialization을 분리하는 후속 구조가 필요하다. 예를 들어 해별로 정확한 symbolic coverage root와 가중치를 보관하고 큐 목록은 요청할 때 순회할 수 있다. 다만 현재 root ID는 worker-local arena에 속하므로 숫자만 결과에 내보내서는 안 된다. 요청의 source universe·규칙·hold 문맥과 함께 arena 수명 또는 안정된 canonical 직렬화를 보장하고, 같은 identity의 여러 root를 OR로 합쳐야 한다. Omit의 membership shortcut으로 생략한 coverage를 이미 알고 있다고 취급해서도 안 된다.

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/result.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage.rs`, `crates/clearra-core-executor/src/solution_probability.rs`. 구현 내용: 해별 scalar 확률, 수명 보장된 symbolic 큐 집합, 큐 페이지 생성의 비용을 분리하고 Include의 기존 의미와 차등 검증한다.

### 후보 1: ILC domain을 BuildUp operation bitmap으로 올리기

candidate의 concrete operation마다 A(p)를 알고 있으므로 D별 허용 operation bitset을 미리 만들거나 lazy하게 계산할 수 있다. 기존 loop 앞에서 다음 교집합을 취한다.

```text
eligible(E) = unplaced(E)
              AND existing_permitted_operations(E)
              AND ilc_allowed_operations[D(E)]
```

연결점은 `PlacementConditionCatalog::exact_clear_domain`, `CandidateProjection::expected_deleted_rows`, `BuildOrderGraph::build_with_legal_board_admission`의 `permitted_operations`다. 허용되지 않는 operation은 개별 realization iterator에 들어가기 전에 제외한다. Unsupported는 기존 mask를 유지한다.

정확한 D를 이미 아는 단계라 증명 부담은 작다. 다만 기존 precomputed instantiation table도 빈 slice를 빠르게 반환하므로 bitset 준비 비용을 포함한 실익을 측정해야 한다. 전체 64개 D를 candidate마다 무조건 계산하는 것보다 실제 방문한 D만 memoize하는 편이 나을 수 있다.

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs`. 구현 내용: concrete operation별 exact domain을 D별 허용 mask로 연결하고 현재 허용 mask와 교집합으로 적용한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/graph_probability_integration_tests.rs`. 구현 내용: 같은 kind의 여러 placement, 여러 ILC 대안, 동시에 지워지는 행, 지원 범위 밖 fallback에 대해 기존 전체 해와 해별 큐를 비교한다.

### 후보 2: APDP의 parent domain에 가능한 D 집합을 전달하기

부분 선택 X에서 가능한 모든 완성과 실행 순서가 만들 수 있는 삭제 행 집합의 과대근사 D_hat(X)를 유지한다. parent p의 exact domain A(p)와의 교집합이 비어 있을 때만 제외한다.

```text
A(p) INTERSECT D_hat(X) == empty  =>  p를 제외할 수 있음
```

아직 선택되지 않은 contributor는 Unknown으로 남겨야 한다. X의 선택을 실제 실행 E로 취급하거나 partial contributor를 이미 완전한 row 조건처럼 다루면 잘못된 prune이 생긴다. 과대근사의 soundness와 원자적 다중 행 삭제가 먼저 증명되어야 한다.

연결점은 `ExactArmPairIndex::parent_rows`, `DomainPropagation::compile/row_allowed`, `FamilyCompiler::advance`다. 현재 residual memo key는 `(remaining, packed_counts)`뿐이다. 새 조건이 앞서 선택한 concrete placement 구성에 의존하면 정확한 condition state를 key에 넣거나 현재 key만으로 결정되는 보수적 조건임을 증명해야 한다.

빈 필드 P7P4에 적용하려면 `compile_minimum`의 공통 필터에도 연결하거나 advanced domain 적용 정책을 별도 실험해야 한다. 기존 APDP block 안에만 추가하면 빈 필드 경로에는 작동하지 않는다. candidate 수를 앞에서 줄일 잠재력은 크지만 전파 비용과 residual memo 공유율 손실도 함께 측정해야 한다.

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_domain.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_apdp.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs`. 구현 내용: partial parent 후보에 보수적인 가능한-D domain을 전달하고 memo의 조건 의존성을 명시한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_parallel_tests.rs`. 구현 내용: incomplete contributor를 Unknown으로 유지하는 API와 동일 remaining·count의 다른 선택 이력, 빈 필드 compile_minimum 경로의 차등 검증을 추가한다.

### 후보 3: Relation guard를 실행·줄 지우기 사건에 따라 갱신하기

현재 보관하는 `influencing_placements`를 실제 reverse incidence/watcher에 연결하거나, guard만으로 충분한 경로에서는 목록 생성을 지연한다. 첫 구현은 D가 바뀌지 않는 전이에 한정할 수 있다. 추가된 physical cell mask가 기존 guard와 서로소이면 같은 target·frame의 증명을 유지한다.

줄 지우기가 발생하면 동일 contributor 그룹의 여러 행을 한 사건으로 처리하고, 기존 frame 증명은 Unknown으로 돌린다. D 사이에 증명을 옮기는 최적화는 별도의 재검증 규칙을 정의한 뒤 확장한다. 관측하지 못한 관계를 독립으로 가정하지 않는다.

연결점은 `PlacementDependencyCache::lookup/observe_*`, `PlacementConditionCatalog::influencing_placements`, `RowConditions::clear_delta`, `CandidateProjection::confirm_transition`이다. 기존 보드 memo를 복제하는 대신 primitive reachability를 건너뛰는 비율을 높이는 것이 목표다. 높이 6 이하에서 짧은 row loop 하나를 줄이는 효과만으로 큰 개선을 예상해서는 안 된다.

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_dependency_cache.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs`. 구현 내용: same-D 전이의 guard 유효성 갱신, concrete ID watcher 또는 영향 목록 지연 생성, atomic clear event의 invalidate 경로를 연결한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_dependency_differential_tests.rs`. 구현 내용: footprint 밖 추가, 먼저 실패하는 kick 후보, 제거·압축되는 행, target/profile/owner 변경, 관측 누락과 admission 종료를 정확 primitive와 비교한다.

## 10. 후속 판단 기준

우선 후보 1로 작고 정확한 API 연결을 검증하고, 후보 3에서 primitive 절감량과 증명 유지 비용을 비교한다. 후보 2는 candidate 수를 앞단에서 줄일 수 있지만 memo 상태와 부분 선택 의미를 함께 바꾸므로 별도 정합성 실험으로 다룬다.

성공 기준은 동일한 해·큐·확률 의미를 유지하면서 전체 search 시간이 감소하는 것이다. cache hit 수, 생성 relation 수, 압축된 domain 수, 특정 stage 시간 감소만으로 성공을 판단하지 않는다. 특히 live/language fusion은 비용을 coverage stage에서 BuildUp stage로 옮길 수 있으므로 전체 wall time과 전체 primitive 상태 수를 함께 읽어야 한다.

# 의존성 순환, 물리 실행 언어, 원본 큐 교집합의 정확성

## 범위와 완료 기준

2026-11-23 사용자 정정에 따라 Full 6L의 완료 기준은 전체 후보 영역 소진,
정확한 정규화 해답 집계, 원본 큐 확률 계산이다. 원본 큐 coverage 100%를 먼저
얻었다는 사실은 전체 해답 열거의 완료 증거가 아니다.

검토 대상 입력은 빈 10×6, SRS+, 15 locks, 유한한 `P1P7P7`, 초기 빈 Hold,
FullQueueOracle이다. 원본 큐는 `7 × 5,040 × 5,040 = 177,811,200`개이며
첫 atom의 한 draw와 뒤의 두 7-bag를 실제 순서 그대로 해석한다. EOF에서는
이미 보유한 held piece만 마지막으로 사용할 수 있고, 보이지 않는 다음 bag의
piece를 새로 공급하지 않는다.

이 문서는 현재 구현의 증명 범위와 회귀를 소스에서 검토한 기록이다. 일반적인
정적 의존성 그래프가 비순환이라는 사실만으로 Full 6L의 물리 실행 가능성이나
원본 큐 교집합의 비어 있지 않음을 증명하지 않는다. 11미노 see-7의 causal policy
문제는 이 검토 범위에 포함하지 않는다.

## 1. 구분해야 하는 세 관계

고정된 기하 후보 `C`에 대해 다음을 구분한다.

| 관계 | 상태와 의미 | 성공 또는 거절에 사용할 수 있는 근거 |
| --- | --- | --- |
| 정적 필수 선행 관계 | 모든 물리 실행이 지켜야 하는 concrete operation 간 순서 | 필수 선행 관계의 순환은 불가능성의 충분조건이다. 비순환만으로 물리 실행을 증명하지 않는다. |
| 물리 실행 관계 `G(C)` | 실행한 operation subset, 정확한 원본 줄 삭제 frame, 실제 SRS lock transition | root에서 accepting terminal로 가는 경로가 있어야 한다. 각 edge가 operation 하나를 추가하므로 이 그래프 자체는 항상 DAG다. |
| 원본 source/Hold 교집합 | 물리 관계의 kind-word 언어와 각 원본 큐의 실제 Hold/EOF 실행 언어 | 같은 원본 ordinal에서 둘을 모두 만족하는 word가 있을 때만 그 큐가 후보를 성공시킨다. |

`BuildOrderGraph::from_topological_parts`는 edge의 도착 index와 depth 증가를
검증한다. 이 일반 생성자 자체는 물리 reachability를 증명하지 않는다. 물리
권한은 `BuildOrderGraph::build`의 정확한 transition 생성이나 별도로 검증된
생산자에게 있다. 따라서 테스트용으로 만든 live chain을 물리적으로 실행 가능한
실제 물리 배치라고 해석해서는 안 된다.

관련 소스:

- [buildup.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs)
- [placement_poset.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/placement_poset.rs)
- [realization_feasibility.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/realization_feasibility.rs)

## 2. 현재 코드에서 성립하는 동치

`L_phys(C)`를 정확한 물리 실행 관계의 accepting path에서 읽은 kind-word들의
집합으로 둔다. `H(q)`는 고정된 원본 큐 `q`, 초기 Hold, 실제 finite EOF 계약으로
만들 수 있는 출력 kind-word들의 집합이다. 요청된 FullQueueOracle의 후보 판정은
다음 관계다.

```text
candidate C succeeds on original queue q
    iff L_phys(C) ∩ H(q) is nonempty.

candidate C is buildable for the original universe U
    iff some q in U has a nonempty intersection above.
```

첫 번째 동치는 정확한 물리 관계를 사용하고, 실제 source와 Hold/EOF 계약을
모두 보존한다는 조건을 가진다. geometry-only transition, 정적 predecessor
상위 집합, 다른 source의 동일 cardinality는 이 조건을 대신하지 못한다.

`CountUnique`의 전체 결과는 이 판정을 통과한 모든 정규화 후보 identity의
집합이다. 한 후보가 하나의 큐에서 성공했다는 양성 증거는 그 후보의 membership을
증명하지만, 다른 후보들의 판정이나 전체 geometry domain 소진을 증명하지 않는다.
`CountAll`의 operation/실행 경로 multiplicity는 별도 그래프와 weighted product의
권한이며, Boolean kind-language quotient로 바꾸지 않는다.

### CertifiedPlacementPoset에만 적용되는 추가 동치

`placement_poset.rs`는 먼저 모든 물리 실행에 필요한 선행 관계를 도출한 뒤,
그 poset의 모든 available ideal transition이 grounded하고 정확한 ILC realization과
이동 증거를 갖는지 별도로 증명한다. 두 방향을 모두 증명해
`Some(CertifiedPlacementPoset)`을 반환한 경우에만 valid operation orders가 그
poset의 linear extensions와 정확히 같다.

이 인증에서도 source/Hold 교집합은 별도로 필요하다. `compile`의 `None`은
지원하지 못했거나 증명을 완성하지 못했다는 fallback이며, 빈 물리 언어라는
결론이 아니다. 반면 `CertifiedConditionalLanguage.root == None`은 해당 생산자가
완결된 관계에서 증명한 빈 언어다. 두 종류의 `None`을 같은 뜻으로 사용하면 안 된다.

## 3. ILC 의존성 closure의 정확한 단방향 계약

`dependency_relaxation_stalls`는 단순한 binary edge cycle 검사가 아니다.

1. operation은 concrete ILC realization 중 하나라도 필요한 줄 삭제 조건을
   만족하면 closure에 들어갈 수 있다. realization 대안은 OR다.
2. 한 원본 row는 그 row에 기여하는 모든 operation이 closure에 들어간 뒤에만
   삭제되었다고 간주한다. row contributor 조건은 AND다.
3. 더 넣을 operation이 없는 least fixed point가 전체 operation에 도달하지
   못하면 relaxed relation도 완료될 수 없다.

이 closure는 collision, grounding, 실제 이동을 완화한다. 모든 실제 실행이
closure 안에 포함되므로 **stall이면 실제 실행과 가능한 원본 큐도 없다**.
closure가 전체에 도달했을 때는 정확한 물리 및 source 검사를 계속해야 한다.
한 realization의 순환을 모든 realization에 대한 불가능성으로 바꾸거나 OR 대안을
필수 AND edge로 합치면 이 증명은 유지되지 않는다.

기존 회귀 `relaxed_ilc_rejects_the_two_row_dependency_cycle`은 서로의 원본 row
삭제를 요구하는 두 operation의 stall을 검사한다.
`relaxed_ilc_accepts_an_acyclic_dependency_chain`은 시작 가능한 operation이 있는
chain에서 stall을 반환하지 않는지 검사한다. 후자는 물리 실행 성공 테스트가 아니다.
현재 optional complete realization prepass는 제품 기본값에서 비활성이고,
비활성 반환은 `Unknown`이다. plain product의 정확한 BuildUp이 최종 권한을 가진다.

## 4. 저장소에 있는 구체적인 반례

다음 테스트들은 [structural_graph_probability_tests.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/structural_graph_probability_tests.rs)에 있다.

### Grounding OR는 하나의 predecessor conjunction으로 표현되지 않는다

`disjunctive_grounding_is_unknown_and_never_an_infeasibility_prune`는 서로 다른
두 O placement가 위의 I를 각각 지지할 수 있는 후보를 검사한다. 물리 kind-word에는
`OOI`, `OIO`가 있고 `IOO`는 없다. concrete operation 기준으로 I는 두 O 중
하나를 필요로 하므로 단순 AND 선행 관계로 전체 관계를 표현할 수 없다.
poset 생산자는 이 경우 `None`으로 fallback한다.

같은 테스트의 roof fixture는 수직 hard-drop corridor가 막혀 있어도 실제 kick
oracle이 `OI`와 `IO`를 모두 허용하는 경우를 포함한다. hard-drop 인증 실패는
실제 이동 불가능성의 증거가 아니다.

### 줄 삭제 뒤의 이동 조건은 추가 선행 관계를 만들 수 있다

`clear_can_close_a_translation_entry_and_requires_a_movement_predecessor`의
10×3 fixture는 다음과 같다.

```text
initial = 0x3f1fc3f7
T cells = 0x201c00
L cells = 0xc02008
```

이 테스트는 SRS+, SRS90, SRS-X, Jstris180, NoKick의 실제 oracle에서 물리 언어가
정확히 `{TL}`임을 검사한다. L을 먼저 놓아 원본 row를 지우면 T의 진입로가 닫힌다.
grounding/row contributor 사실만으로는 필요한 `T < L` 이동 조건을 얻지 못한다.

이 검사 결과와 Hold 규칙에서 바로 따라오는 별도 예는 source가 고정 큐 `LT` 하나이고
Hold가 금지된 경우다. 물리 DAG에는 accepting path `TL`이 있지만 source/Hold
교집합은 비어 있다. 이것은 일반적인 비동치의 예이며, 빈 Full 6L/Hold 허용 입력에
한정한 기하 반례라고 주장하지 않는다.

### 같은 compact board도 같은 ILC 상태가 아니다

`identical_compact_board_changes_pending_i_lock_across_original_clear_frames`는
SRS90에서 compact board `0x3`가 같아도 원본 삭제 frame이 `0`과 `0b011`일 때
같은 pending I operation의 합법성이 달라지는 것을 검사한다. 원본 row 2의 I가
나중에는 floor의 pose로 투영된다. 따라서 compact occupancy 또는 pairwise
dependency만으로 정확한 상태를 합칠 수 없다.

`one_i_operation_keeps_opposite_ordered_kick_answers_for_its_two_realizations`는
같은 I lock mask의 두 rotation realization이 각각 불가능/가능한 사례를 검사한다.
한 representative realization의 실패를 operation 전체의 실패로 넓힐 수 없다.

## 5. 빈 물리 언어와 빈 source 교집합의 전달

현재 [buildup.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs)의
ordinary verifier는 `graph.nodes[graph.root].live`일 때만 coverage product를
실행한다. dead root이면 coverage가 없고 최종 `buildable=false`다.

[piece_order_language.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/piece_order_language.rs)의
`canonicalize_and_mark_live`는 dead root에 `Ok(None)`을 반환한다.
`fused_language_dead_root_cannot_reuse_previous_graph_classes`는 동일 owner에서
live → dead → live를 실행해 이전 graph class가 dead root를 살려내지 못하게 한다.
`conditional_language_exact_empty_and_dead_branches_are_not_unknown`은 완결된
조건부 관계의 빈 언어와 일부 dead branch의 의미를 구분한다.

live kind language가 source와 만나지 못하면
[standard_bag_coverage.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage.rs)의
정확한 product가 zero original coverage와 no witness를 반환해야 한다.
`certified_language_result`는 `covers_any_pattern`이 false인 경우
`buildable=false`, `symbolic_coverage_root=None`을 유지한다.

`covers_any_pattern`의 공통 계약은 실제 materialized original coverage count가
0이 아닌지로 판단한다. truncated-prefix source에서는 symbolic full source의
non-REJECT root만으로 retained ordinal의 존재를 주장할 수 없다.
`truncated_standard_bag_prefix_cannot_make_a_candidate_buildable`이 이 consumer
경계를 검사한다. complete factorized source는 전체 원본 ordinal을 보존하므로
이 차이가 나타나지 않지만 같은 공통 계약을 사용한다.

## 6. Factorized source의 pointwise 계약과 추가 회귀

[factorized_source.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage/factorized_source.rs)는
실제 complete uniform backing에서만 layout을 얻는다. atom의 choice 순서,
draw 수, 경계 reset, suffix count, full/visible 길이를 보존한다. 입력 문자열이나
같은 pattern count는 source authority가 아니다.

`StandardBagCoverage::solve`는 current, swap-held, store-current의 실제 공급
transition과 kind edge를 함께 소비한다. finite factorized EOF에서는 held source
piece의 마지막 사용만 허용하고 Standard7Bag의 unseen complement 분기를 차단한다.

관련 테스트는 [factorized_tests.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage/factorized_tests.rs)에 있다.

| 테스트 | 확인하는 범위 |
| --- | --- |
| `factorized_coverage_matches_every_original_queue_and_canonical_hold_branch` | `P1P4`, `P2P3`, `P1P6`, `[LOI]2P5`의 모든 원본 ordinal을 canonical finite supply automaton과 비교. Hold 금지/빈 Hold/held T를 포함한다. |
| `factorized_partial_atom_eof_cannot_place_its_unseen_complement` | finite partial atom EOF에서 미공개 complement를 사용할 수 없고 실제 held piece만 사용할 수 있다. |
| `factorized_p1p7p7_rank_boundaries_keep_exact_witnesses_without_expanding_source` | Full P1P7P7의 대표 원본 rank 경계 3곳에서 정확한 witness를 유지한다. 전체 177M 큐 전수 회귀라고 해석하지 않는다. |
| `factorized_live_language_empty_intersection_matches_all_108_original_queues` | 아래 108개 큐를 실제 factorized `P1P3P3`의 동일 원본 ordinal에 연결하고, negative/positive/negative를 같은 owner에서 실행한다. 전체 bitmap, count, witness와 나머지 ordinal의 음성 결과를 비교한다. |
| `factorized_p1p7p7_live_matching_inventory_language_can_have_no_source_queue` | matching inventory인 15-kind live chain도 Full P1P7P7/Hold 교집합이 비어 있을 수 있음을 symbolic count와 witness로 검사한다. |

### 108개 원본 큐의 명시적인 empty intersection

source `[IOT]1[IOT]3[IOT]3`은 `3 × 6 × 6 = 108`개 원본 큐를 갖는다.
live acyclic kind chain `IIIOOTT`는 그중 첫 draw가 I인 큐와 같은 inventory다.
그러나 세 번째 I를 만나기 전에 첫 3-piece bag의 non-I 두 개를 통과해야 한다.
초기 빈 Hold는 한 개만 미룰 수 있으므로 처음 세 locks를 `III`로 만들 수 없다.

새 회귀는 기존 `finite_oracle`의 canonical `SupplyExecutionAutomaton::sequence`
transition으로 108개를 모두 검사한다. 실제 source의 ordinal 0을 kind chain으로
만든 positive control도 비교한 다음 동일 negative root를 다시 검사한다. 하나의
language owner에 두 root를 추가하여 numeric root ID의 namespace를 보존한다.

제품 parser는 4,096개 이하의 source를 explicit storage로 보관한다. 따라서 이
108개 source 자체가 factorized factory를 지원한다고 가정하지 않는다. 테스트는
실제 factorized `P1P3P3`의 308,700개 원본 중 동일한 길이 7 큐의 ordinal을 찾고,
각 mapping을 실제 `sequence_at` 결과와 대조한다. 두 chain은 I/O/T만 사용하고
모든 draw를 소비하므로 나머지 원본 큐는 성공할 수 없다. 108개 canonical oracle로
만든 expected bitmap을 product의 전체 bitmap과 비교하여 이 나머지 영역도
음성임을 확인한다. Hold나 finite EOF 규칙, 원래 108개 큐의 내용은 바꾸지 않는다.

### Full P1P7P7의 matching-inventory 음성 사례

`IIIOOTTSSZZJJLL`은 15개이며 I가 3개, 나머지 종류가 각각 2개다. 이는 P1에서
I를 선택한 source inventory와 일치한다. 그러나 세 번째 I는 두 번째 7-bag까지
도달해야 나오므로 같은 `III` prefix가 불가능하다.

새 회귀는 이 nonempty chain의 product가 `REJECT`, count 0, witness None인지
검사한다. ordinal 0의 실제 source word를 positive control로 사용하고 negative를
다시 방문한다. 177,811,200-bit bitmap은 materialize하지 않는다.

두 chain 회귀는 **source/Hold 교집합**의 증거다. 해당 chain만을 갖는 실제 Full 6L
기하 후보를 구성하거나, 모든 Full 6L 기하 후보의 성질을 증명한 것은 아니다.

## 7. 실행 검증 상태

`d9225aa1`의 통합 factorized 테스트는 **8/8 통과**했다. 첫 실행에서는 작은
108개 source의 explicit storage를 factorized factory에 넣는 fixture 오류로
7/8이었으며, 제품 factory를 변경하지 않고 위의 실제 `P1P3P3` ordinal 연결로
고쳤다. 실제 Full P1P7P7의 negative/positive/negative와 수정된 108개 원본 전수
대조를 모두 실행했다. 최종 로그는
[exhaustive-d9225aa1-factorized-source.log](../../_local/artifacts/full6l-p1p7p7-20261122/exhaustive-d9225aa1-factorized-source.log)다.

별도로 `e6301cdb`의 실제 factorized P6(5,040개 원본, 6 locks)의 serial/parallel
결과에서 전체 normalized identity 배열, 전체 원본 bitmap, count/hash, raw candidate
count/digest와 family count가 일치했다. consumed-candidate 취소 후 새 control로
재개해도 count/probability 완료 권한을 얻지 못하는 회귀도 통과했다. 이 검증들은
Full 6L 전체 후보를 완주했다는 뜻이 아니다.

후속 최적화는 exact candidate set과 원본 큐 교집합을 보존해야 한다. 정적 closure
stall, 완결된 물리 언어의 공집합, 완결된 source product의 공집합만 해당 범위의
음성 증거로 사용한다. 취소, 자원 거절, 인증 fallback 또는 coverage 조기 포화는
전체 해답 영역 소진의 증거로 사용하지 않는다.

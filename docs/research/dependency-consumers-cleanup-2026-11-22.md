# 의존성 소비자 확대와 중복 계산 정리

작성일: 2026-11-22

소스 기준: `fafa9a1` 위의 코드 커밋 `fa199dfc5873322aa457615248b384b6d91d79cc`.
작업 브랜치: `codex/dependency-consumers-cleanup-20261122`.
실행 검증 결과는 아래 통합 검증 절에 기록한다. 후속 문서 커밋은 이 코드 상태를 변경하지 않는다.

## 1. 변경 목적과 범위

정확한 고정 타일링 의존성 관계가 이미 만든 결과를 공급·Build 완료·관측 소비자가 다시 계산하지 않고 사용할 수 있도록 연결한다. compact Boolean 소비자에서 필요한 결과는 각 원본 큐에 대한 성공 여부다. 이 결과를 위해 정확한 종류 언어를 얻은 뒤 동일한 BuildUp 그래프를 다시 만들 필요는 없다.

이번 변경은 다음 네 계산을 공유한다.

1. concrete 그래프와 의존성 종류 언어가 같은 `CoverageProductEvaluator`의 공급·Hold 전이 커널을 사용한다.
2. PC의 빈 보드 완료와 Build의 정확한 비영 보드 완료가 같은 원래 셀·행 삭제 projection과 operation 관계를 사용한다.
3. 같은 operation 관계가 이미 계산한 suffix 언어를 같은 arena 안에서 재사용한다. 각 후보의 물리 좌표는 별도로 보존한다.
4. forward-search와 spin-structure-search의 회전 원점 변환을 `clearra-rules`의 기존 구현으로 통일한다.

전체 see-7 실행이나 P7P4 성능 개선의 완료 보고가 아니다. 이 문서의 동치 주장은 아래 계약에 대한 논증이며, 테스트 성공만으로 모든 경로의 동치를 대신하지 않는다. 선행 모형은 [고정 타일링 ILC 의존성 증명](fixed-tiling-ilc-dependency-proofs-2026-11-22.md)과 [보드 family·ordered kick 구성](combinatorial-board-family-proof-2026-11-22.md)을 따른다.

## 2. 소비자별 적용 경계

`placement-dependency-pc` feature의 제품 경로를 기준으로 한다. 기존 `precompute_build_dependencies=false`는 적격 요청의 자동 선택을 끄는 설정이 아니다. 명시적 진단 reference는 테스트 또는 기존 native A/B 경로에서만 유지한다.

| 소비자 | 이번 연결 | 유지하는 경계 |
| --- | --- | --- |
| compact PC, Boolean `CountUnique` | 폭 10, 1~6L, 실제 required cells의 exact cover 1~11 operation, materialized 공급에 적용 | 원래 보드 영역·고정 horizon·목표·output/evidence 적격 조건을 계속 검사 |
| fixed sequence·queue expression 등 materialized 공급 | 기존 원본 ID bitset 공급 product를 종류 언어에서 직접 사용 | fresh standard bag이라고 가정하지 않음. 원본 universe, Hold, cursor와 lookahead 계약 유지 |
| 완전 standard bag | 기존 `StandardBagCoverage` 표현을 우선 재사용 | fast representation의 자체 지원 조건을 만족할 때만 사용 |
| compact Build의 정확한 최종 보드 | `ExactBoardAfterLineClears(board)`를 같은 의존성 관계에 전달 | 폭 10, 1~6L·1~11 operation, `CountUnique`, materialized 공급, finesse·spin coverage·score·실행 제약 없음. 자동 확대 경로는 oracle 소비자에 한정 |
| compact PC `VisibleSeven` | 의존성은 factored owner, concrete fallback은 raw graph에서 물리 action을 직접 등록 | 종류가 같다는 이유로 서로 다른 lock을 합치지 않음. full-height 관측은 별도 잔여 경계 |
| CountAll | 기존 concrete producer 유지 | 종류 언어는 물리 경로·Hold 행동의 다중도를 보증하지 않음 |
| finesse·spin·점수·실행 evidence | 기존 concrete graph와 evidence 생성 경로 유지 | 비용, 회전·kick 이력, 구체 실행 identity를 Boolean 결과에서 복원했다고 간주하지 않음 |
| Setup Finder | 기존 `PartialBuildGraph`·`SetupCoverageGraph` 경로 유지 | residue·terminal supply·부분 경로·joint 조건을 포함한 Setup 전체를 이번 Build 연결로 대체하지 않음 |
| extended PC/Build, 범위 밖 높이·operation 수 | 기존 extended/범위 밖 verifier 유지 | Board64 지원 상한을 늘리는 변경이 아님 |
| 명시적 실행 예산 | 기존 admission 및 dispatch 경계 유지 | PC의 max nodes/candidates/frontier 또는 명시적 memory cap은 자동 의존성 적격에서 제외. Build 자동 확대도 명시적 memory cap이 있으면 제외 |

PC 적격의 최종 권위는 `SearchProblem::pc_placement_dependency_eligible`이다. Build 생성자와 `verify_candidate_for_completion_mode`는 `uses_build_placement_dependency`라는 동일 scope를 공유한다. `ClearToEmpty`에만 PC 적격을 적용하고 `ExactBoardAfterLineClears`에는 Build 적격을 적용하므로, 기반 `SearchProblem`이 PC 적격이어도 별도 finesse·spin aggregation이 Boolean 경로로 들어가지 않는다. spin aggregation의 초기 일반 Build pass에도 그 의도를 전달한다. 지원 후보의 크기 확인은 `placement_dependency::supports_candidate`가 소유한다. 크기 검사만 통과했다고 이동·공급·evidence의 정확성이 승인되는 것은 아니다.

공개 Build의 imported profile 지원은 이번 범위에 포함하지 않는다. `checked_build_probability_pointee_retained_bytes`가 아직 해당 profile의 중첩 할당 owner를 산정하지 않으므로 기존의 명시적 admission 거절을 유지한다. 내부 적격 Build 생성자가 imported table을 받는 경우에도 PC에서 유도한 높이나 같은 이름의 builtin table을 대신 쓰지 않도록 실제 Build catalog의 폭·높이로 승인하고 workspace에 결박한다.

## 3. 공급 커널 재사용의 모든 경로 동치

### 3.1 비교하는 결과

한 고정 후보의 정확한 종류 언어를 `L ⊆ Σⁿ`이라 하자. 원본 materialized universe의 ID 집합을 `Ω`, ID `p`가 나타내는 큐를 `qₚ`, 고정 초기 공급/Hold 상태를 `h₀`라 한다. 성공 ID 집합은

\[
C(L)=\{p\in\Omega : \exists w\in L,\ \exists\text{ 기존 공급/Hold 전이열 }h_0\xRightarrow[q_p]{w}\mathrm{accept}\}.
\]

따라서 concrete producer와 의존성 producer가 같은 고정 길이 `L`을 제공하면, 같은 공급 전이 관계에 대한 존재 양화 결과 `C(L)`도 같다. 물리 실현이 여러 개여도 동일 ID에 대한 Boolean 합집합은 멱등적이다. 이 주장은 `CountAll` 경로 수를 보존한다는 주장이 아니다.

### 3.2 같은 전이를 실제로 공유하는 방법

`coverage_product.rs`의 `BuildGraphLanguage`와 `RootedPieceLanguage`는 `CoverageLanguage`라는 작은 읽기 표면을 제공한다. 전이의 종류와 다음 node만 서로 다른 owner에서 읽고, 두 경로 모두 `evaluate_mode`를 호출한다. 새 언어 전용 Hold 알고리즘을 복사하지 않는다.

언어 adapter는 다음을 검사한다.

- root의 remaining depth가 `n`일 때 node의 실행 깊이는 `n - remaining_depth`다.
- 모든 간선은 remaining depth를 정확히 1 줄인다.
- accepting node의 remaining depth는 0이다.
- CountAll을 요청할 수 없다. 고정 horizon을 위반하는 언어는 오류이며, 성공 없는 큐로 변환하지 않는다.

이 rank를 기준으로 product 깊이에 대해 귀납한다. root에서는 같은 원본 ID word와 같은 Hold 상태가 활성화된다. 깊이 `d`에서 두 표현의 동일 suffix와 Hold 상태에 도달한 ID 집합이 같다고 가정하면, 같은 종류 간선이 동일 current/store/swap/release mask와 교차한다. 결과를 다음 상태에 OR하므로 동일 ID 집합이 유지된다. 깊이 `n`에서 accepting 상태에 도달한 집합도 같다. 이 논증은 성공 경로 하나를 고르는 순서나 hash-table iteration 순서를 가정하지 않는다.

### 3.3 원본 ID, 가중치, cursor

한 ID의 큐 문자열이 다른 ID와 같아도 ID를 합치지 않는다. `PatternPiecePositionIndex`는 후보에 맞는 local ordinals로 좁힐 수 있지만, 결과는 반드시 `expand_coverage_words`로 원본 universe의 ordinal에 되돌린다. 성공한 두 중복 큐 ID의 가중치가 다르거나 하나가 0이어도 각 bit는 별도로 남는다. 확률은 이 최종 원본 ID 집합에 원래 가중치를 합산하여 얻으므로, bitset 동치는 같은 가중치 모델에 대한 확률 동치를 함의한다.

큐 위치는 기존과 같이 `initial_cursor + executed_depth + extra_draw`다. 빈 Hold에 현재 미노를 보관하는 전이는 추가 draw를 소비하며, 이미 점유된 Hold 교환은 같은 초기 cursor와 slot을 사용한다. 마지막 held release와 standard-bag projected lookahead도 기존 guard를 그대로 호출한다. 이미 진행된 cursor를 0으로 정규화하거나, 성공 후보만 보고 숨겨진 마지막 미노를 확정하지 않는다.

`cover_piece_language`는 완전 bag의 symbolic representation과 일반 materialized bitset을 한곳에서 선택한다. 일반 커버리지 cache의 key는 종류 언어 root와 `pattern_index_id`다. 이것이 충분한 이유는 `BuildUpWorkspace`가 하나의 고정 요청 안에서 사용되어 초기 Hold/cursor, 공급·관측 계약과 원본 universe가 바뀌지 않기 때문이다. 이 cache를 다른 요청에 재사용하려면 그 owner 계약부터 확장해야 한다.

`LanguageNodeSlots`는 실제 product가 방문한 언어 node에만 dense slot을 부여한다. arena 전체를 복사하거나 새 BuildUp 그래프로 재구성하지 않는다. 평가마다 slot mapping을 초기화하고 allocation만 재사용하므로, 다른 root나 새 arena의 같은 숫자 ID가 과거 상태를 가리킬 수 없다. slot 저장소도 retained-byte 산정에 포함한다.

## 4. 비영 Build 완료의 정확성

초기 점유를 `B₀`, 서로 겹치지 않는 각 operation의 원래 네 셀을 `Pᵢ`라 한다. `CandidateGeometry::for_symbolic_prefix_completion`은 다음 조건을 유지한다.

- 초기 점유와 required cells가 겹치지 않고 보드 안에 있다.
- 각 operation은 표준 tetromino의 정확한 네 셀이며, required cells 안에 있고 다른 operation과 겹치지 않는다.
- 모든 `Pᵢ`의 합집합이 required cells와 정확히 같다.
- 초기 full row는 정규화 없이 받아들이지 않는다.
- 전체 operation을 배치한 보드를 실제 완료 행들로 compact한 결과가 전달받은 `expected_final_board`와 정확히 같다.

기존 `for_symbolic_prefix`와 poset/conditional 생성자는 여전히 최종 보드 0을 요구한다. 이번 확장은 기존 빈 보드 검사를 삭제한 것이 아니라 완료 보드를 명시적으로 전달하는 생성자를 추가한 것이다.

실행된 operation 집합 `S`에서 원래 셀 합집합과 삭제 행은

\[
U(S)=B_0\cup\bigcup_{i\in S}P_i,\qquad
D(S)=\{r:\ r\text{는 전체 목표에서 full이고 그 contributor가 모두 }S\text{에 속함}\}.
\]

상태 보드는 같은 원래 행 projection `Φ`를 사용한 `Φ_{D(S)}(U(S))`다. 새 operation으로 행이 삭제되지 않으면 같은 frame에서 그 footprint를 OR한다. 삭제가 있으면 원래 실행 셀 합집합을 새 frame에 다시 투영한다. 삭제된 compact 보드에 이전 좌표의 footprint를 그대로 더하지 않는다. 이 귀납은 최종 보드가 빈 보드인지와 무관하므로, 기존 ILC·grounding·movement 명제와 operation 전이는 그대로 사용할 수 있다.

전체 집합 `I`에서는 `Φ_{D(I)}(U(I)) == expected_final_board`라는 생성자 검사가 `BuildCompletion::ExactBoardAfterLineClears`의 완료 조건과 일치한다. 따라서 실제로 해당 목표까지 도달하는 모든 허용 순서를 포함하며, 다른 비영 보드를 잘못 완료로 받아들이지 않는다. 부족한 셀, 잘못된 완료 보드, 지원 밖 후보와 취소·관계 상한 거부는 실패 큐의 확률 0으로 바꾸지 않는다.

대표 경로는 원본 성공 ID를 고른 뒤 같은 operation 관계에서 공급/Hold 선택을 찾고, 선택된 lock의 concrete pose만 복원한다. `buildup/placement_dependency/witness.rs`가 이 역할을 유지한다. 종류 언어 자체를 구체 이동 이력이라고 취급하지 않는다.

대표 선택 순서도 기존 소비자에 맞춘다. 일반 concrete graph는 operation을 먼저 순회하지만 fixed-source witness shortcut은 Current 행동을 Hold보다 먼저 순회한다. 새 witness는 같은 기존 공급 전이로 최대 `3 × MAX_OPERATIONS`개 선택지를 생성하고, shortcut 계약일 때만 공급 우선순위와 operation 순서로 정렬한다. 비초기 Hold에서 서로 다른 두 operation 순서가 모두 가능한 경우에도 해당 대표 선택 규칙을 유지한다.

## 5. suffix 계산 재사용과 arena 소유권

`ExactPrefixRelation`의 의미는 operation 수, 순서가 정해진 operation별 종류, 각 operation의 `permitted_parents`, 그리고 `active_prefixes`가 결정한다. `CompactPrefixRelation::matches`는 이 내용 전체를 비교한다. hash는 검색용 bucket일 뿐 동치의 증거가 아니다.

완료 prefix의 suffix 언어는 `{ε}`이고 다른 prefix의 suffix는 각 live operation의 종류를 해당 child의 suffix 앞에 붙인 합집합이다. operation 수에 대한 역방향 귀납으로, 위의 완전한 labelled relation이 같으면 모든 대응 prefix의 suffix 언어가 같다. 그러므로 이미 등록된 관계에 대한 hit는 bottom-up suffix 계산을 다시 수행할 필요가 없다.

이때 같다고 보는 것은 relation과 종류 suffix다. `register_relation`은 각 후보의 `CandidateGeometry`를 계속 보유한다. 관측 정책이 물리 lock을 요청하면 해당 후보의 original-cell/삭제-row frame으로 action key를 구한다. 같은 kind·suffix를 가진 다른 footprint도 서로 다른 물리 선택으로 유지한다. 준비한 relation handle을 다음 후보에 잘못 붙이지 않도록 등록 시에도 완전한 relation equality를 다시 검사한다.

숫자 suffix ID만 같아서는 재사용할 수 없다. `PieceOrderLanguageCache`의 `Arc<()>` arena identity를 observation owner가 고정하고, 살아 있는 physical root가 있으면 arena 교체를 거부한다. `clear_retain_capacity`는 새 identity를 발급한다. 취소되거나 공급 product에서 탈락하여 아직 physical root를 등록하지 않은 시도는 pin을 유지할 근거가 없으므로 새 arena에 다시 결박할 수 있다. worker merge는 정확한 kind-node import가 만든 ID mapping으로 suffix를 재결박한다.

이 조건은 allocator가 주소를 재사용하거나 arena node 번호가 다시 0부터 시작하더라도 다른 언어를 과거 증거로 소비하지 않게 한다. 이 변경의 성능상 의미는 동일 관계의 suffix 계산·저장 반복을 생략한다는 구조적 사실까지다. 큰 요청의 wall time/RSS 개선 수치는 아직 주장하지 않는다.

## 6. 회전 원점 변환의 단일 소유자

`clearra-forward-search/src/reachability.rs`와 `clearra-spin-structure-search/src/entry.rs`에 중복되어 있던 `normalized_kick_delta`·원점 표를 제거하고 기존 `clearra_rules::kicks::normalized_kick_delta`를 호출한다. 두 crate는 이미 `clearra-rules`에 의존하므로 새 crate나 dependency를 추가하지 않는다.

normalized anchor `a`, 방향별 원점 offset `cᵣ`, raw kick `k`에 대해 새 anchor는

\[
a'=a+k+c_r-c_s.
\]

제거한 함수와 canonical 함수는 같은 정수 연산과 같은 방향 표를 사용한다. J/L/S/T/Z는 `(1,0),(0,1),(1,1),(1,1)`, I는 `(0,0),(-2,2),(0,1),(-1,2)`, O는 모든 방향 `(0,0)`이다. 그러므로 각 개별 kick 후보의 destination이 같다. 기존 순서대로 후보를 검사하므로 first-success 후보, 보존하는 kick index/raw offset evidence와 충돌·천장 판정도 달라지지 않는다.

이 항등식은 특정 no-kick 목록을 가정하지 않는다. 다만 변환 함수를 공유했다고 모든 외부 kick profile의 entry·finite ceiling·evidence 연결까지 새로 승인한 것은 아니다. ordered-kick reachability와 프로필별 지원 조건은 기존 권위를 유지한다. spin-structure 쪽 변경도 좌표 변환 중복 제거이며, 새로운 spin 의존성 탐색을 활성화하지 않는다.

### 6.1 소비자마다 실제로 사용하는 킥 테이블의 권위

Boolean PC가 외부 ordered table을 승인할 수 있다는 사실만으로 다른 소비자가 같은 프로필 이름의 builtin table을 사용해도 되는 것은 아니다. `ordered_kick_authority(problem, width, height)`는 실제 table과 실제 geometry를 결박한다. 이 owner를 소비하지 않는 snapshot·Setup·extended·Tiling 경로는 `ensure_canonical_kick_profile`을 통해 전체 ordered table이 builtin과 일치하는지 확인한다.

기존 PC의 fixed/no-hold 예외는 제출된 전체 큐에서 실제로 나올 수 있는 종류의 ordered entries만 비교한다. 이것은 해당 PC 요청에만 충분하다. snapshot은 큐와 독립적인 placements를 입력받아 graph와 finesse evidence를 만들 수 있으므로 그 예외를 적용하지 않는다. 회귀 fixture는 큐가 O/O일 때 사용하지 않는 T entry의 순서만 바꾸어, whole fixed PC는 기존 좁은 계약으로 승인하고 snapshot은 명시적으로 거절하는 것을 확인한다. 완전히 같은 canonical imported table은 snapshot에서 계속 승인한다.

## 7. compact 물리 관측 fallback의 통합

종류 언어의 Boolean 동치만으로 see-7 정책 동치를 결론내릴 수 없다. 같은 관측 아래에서는 concrete lock을 먼저 선택하고 다음 reveal을 받아야 한다. 같은 종류의 다른 lock을 합친 뒤 미래 정보에 따라 과거 선택을 다시 고르는 효과가 생기면 성공 확률이 잘못 올라간다.

이번 suffix 재사용은 이 물리 선택 owner를 보존한다. 의존성 범위 밖의 compact concrete fallback도 `PhysicalActionLanguage::register_graph`를 통해 raw `BuildEdge`의 `(piece, rotation, x, y)`에서 현재 lock의 네 셀을 직접 가져온다. 이동 탐색을 다시 실행하지 않으며, graph가 recycle되거나 종류 언어로 사영되기 전에 등록한다. 이전의 별도 kind root 저장·후처리 병합은 제거했다.

등록은 root에서 도달 가능한 live 부분만 읽는다. 모든 accepting node의 깊이, 간선의 순방향 rank, width, 네 셀의 Board64 경계를 검사한다. 같은 `(piece, cells)`에 대해서만 자식 residual을 union한다. 원래 graph를 수정하지 않으므로 O의 두 orientation이 같은 physical action으로 합쳐져도 CountAll이 소비하는 raw 간선 두 개는 그대로 남는다. importer와 최종 root union은 동일한 `intern_action_union`을 사용한다.

같은 초기 보드와 규칙에서 동일 physical history는 동일 현재 보드와 line clear 결과를 결정한다. 따라서 history에 대한 귀납으로, 같은 다음 lock을 실행한 뒤 가능한 완성의 집합은 각 child residual의 정확한 합집합이다. 다른 lock끼리는 이 동치가 없으므로 별도 선택으로 유지한다. 기존 관측 평가기의 `max(action) → reveal별 합` 순서에 그대로 연결하면 숨겨진 미래로 과거 배치를 바꾸는 효과가 생기지 않는다.

worker owner 이관도 physical/kind 수명에 맞춘다. factored shard만 대응 kind arena를 함께 이동하고, eager shard는 kind arena를 필요로 하지 않는다. 목적지에서 ID를 다시 등록한 후 하나의 전역 정책을 계산하며, worker별로 최적화한 성공 bitset을 OR하지 않는다. 현재 concrete fallback의 native parallel dispatch gate는 유지되므로 eager fallback 병렬 실행 확대까지 완료한 것은 아니다.

eager view는 workspace를 빌리므로 owner 메모리를 metric에 다시 더하지 않는다. 최종 root union과 policy 평가가 늘린 peak는 기존 configured memory cap에 다시 반영하고, cap을 넘었을 때 완전한 결과로 표시하지 않는다. 부모가 승인한 공유 입력의 경로는 기존 admission에도 그 평가 메모리를 전달한다. 최종 관측 할당 전체에 대한 사전 상한 증명은 이번 변경의 주장이 아니다.

합성 `BuildOrderGraph` 회귀에서는 처음 일곱 종류가 같고 여덟 번째만 다른 두 큐를 사용한다. 서로 다른 첫 I lock을 선택해야 하면 성공은 두 큐 중 하나이며, 동일한 첫 lock 뒤의 residual이면 두 큐를 함께 처리할 수 있다. oracle 공급 product는 두 큐를 모두 허용한다. 이것은 실제 4L 보드에서 찾은 반례가 아니라 importer와 관측 선택 시점을 겨냥한 유한 관계 검산이다.

탐색할 미노가 모두 보이거나 원본 공급 계약으로 마지막 하나가 확정되어 숨겨진 정보가 남지 않으면 oracle과 같은 언어 존재성 평가를 사용할 수 있다. 그러나 후보 성공 집합에서 역으로 추론하여 숨겨진 정보를 줄이면 안 된다. 8미노부터 생길 수 있는 실제 관측 차이와 10~11미노 기본 workload의 완전한 실행은 별도 잔여 작업이다.

## 8. 제거한 중복과 아직 필요한 BuildUp 소비자

일반 materialized 커버리지를 `verify_candidate_with_projection` 안에서 별도로 계산하던 분기는 `cover_piece_language`로 통합한다. 새 dependency 경로도 같은 함수를 사용한다. 따라서 language cache hit 처리, 원본 ID 확장, witness ID 선정, bag/bitset 표현 선택에 두 구현을 유지하지 않는다. 적격 compact Boolean 후보는 증명한 종류 언어에서 공급 product를 직접 실행하고 graph node를 만들지 않는다.

남은 Boolean graph 소비자도 `piece_language_projection_only && CountUnique && Existing reachability`이면 기존 `canonicalize_and_mark_live`를 사용한다. 완성 가능 node 표시와 suffix 언어 생성을 같은 역방향 순회로 계산하고, 반환된 request-local root를 후속 공급 product가 즉시 소비한다. CountAll·finesse·구체 snapshot은 이 사영을 선택하지 않는다. test/native A/B의 기존 축에는 분리된 두 순회 reference를 남겨 독립 비교를 유지한다.

기존 `MembershipOnly` shortcut은 이미 전체 union에 포함된 singleton target의 coverage row를 생략했다. 의존성 소비자는 해당 후보의 정확한 singleton coverage를 반환하므로 전체 성공 ID 집합과 unique count가 같아도 `coverage_row_count`·`pattern_verified_execution_count` 같은 작업 진단값은 증가할 수 있다. 모든 진단 필드까지 같다는 주장은 하지 않는다.

명시적 cap의 compact graph fallback은 기존 보수적 candidate preflight를 유지한다. 그 상한은 Boolean 경로에도 CountAll의 64개 `u128` counter를 포함하여 subset당 16,704 bytes의 product 공간을 계산하고 마지막에 재할당 공존 계수 2를 적용한다. 이번 lazy node-slot mapping은 이 여유 안에 들어간다. arena identity의 `Arc<()>` 제어 블록도 cache에서 한 번 계상한다.

반면 다음 소비자는 여전히 구체 정보가 필요하므로 삭제 대상으로 묶을 수 없다.

| 남은 소유자·호출 경로 | 보존해야 하는 계약 | 후속 정리 조건 |
| --- | --- | --- |
| `CoverageProductEvaluator::evaluate`, concrete `BuildOrderGraph::edges` | CountAll의 물리 실현·Hold 행동 다중도 | 관계에 같은 multiplicity 의미를 명시적으로 보존하기 전까지 유지 |
| `evaluate_with_finesse`, `PreparedFinesseNode/Language` | spawn, 입력 비용, 구체 자세와 실현 | Boolean suffix가 지우는 정보를 별도 정확 owner로 제공해야 함 |
| `result.rs`의 `exact_scoring_execution_graph` | PC 점수·회전/spin·B2B 등 concrete 실행 evidence | 해 집합/커버리지 동치만으로 삭제 불가 |
| `build_probability.rs::prepare_exact_spin_execution_batch` | 정확한 Build 완료 보드에 대한 spin 실행 evidence | 완료 보드 확장과 evidence 확장을 별개로 검증 |
| `extended_build_probability.rs`, `extended_buildup.rs` | Board256·더 높은 보드의 exact build graph | compact 1~6L proof 지원을 전체 높이 지원으로 확대 해석하지 않음 |
| `extended_pc_observation.rs` | full-height 관측의 현재 kind-only union | Board256 물리 action key와 해당 owner의 사전 메모리 상한을 마련하여 별도로 전환해야 함 |
| `setup_finder.rs`, `setup_partial_build.rs`, `setup_coverage_graph.rs` | 부분 타일링, residue와 terminal supply, joint/all-path 결과 | 단일 고정 완료 보드의 Boolean 계약에 해당하는 내부 연산부터 별도로 재사용 |
| 예산 제한 요청과 명시적 reference | 기존 admission, truncation/evidence 및 차등 검증 권위 | 새 경로의 동일 자원·완료 계약을 입증한 뒤 별도 이전 |

`BuildUpWorkspace`라는 이름의 소유자에는 이제 종류 언어·공급 cache·관측 owner도 들어 있다. 이름이 남아 있다는 사실과 실제 BuildUp 그래프를 생성한다는 사실을 구분해야 한다. 단순한 파일 삭제나 wrapper 교체보다 소비자가 사용하는 계산과 증거를 기준으로 정리한다.

## 9. 통합 검증 기록

아래 실행은 위 코드 커밋과 동일한 소스에서 Rust 1.98.1, committed lockfile로 완료했다. 코어 suite는 Rust `clearra-manage`의 `build-test` profile, 900초 timeout으로 감독했다. libtest 자체를 `--test-threads=1`로 실행하여 전역 admission fixture가 충돌하지 않도록 했으며, 테스트 안에서 요청하는 solver worker 수는 바꾸지 않았다.

| 실행 | 결과 |
| --- | --- |
| `cargo test --locked -p clearra-core-executor --features parallel,local-search-ab,placement-dependency-pc --lib -- --test-threads=1` | **1,064 통과 / 0 실패 / 13 ignored**, 테스트 시간 168.72초. baseline 대비 새 회귀 20개 포함 |
| `cargo test --locked -p clearra-problem -p clearra-forward-search -p clearra-spin-structure-search --lib -- --test-threads=1` | problem **118 통과**, forward-search **135 통과 / 2 ignored**, spin-structure-search **47 통과**. 실패 0 |
| `cargo check --locked -p clearra-core-executor` | 통과, 경고 없음 |
| `cargo check --locked -p clearra-core-executor --no-default-features --features parallel` | 통과, 경고 없음 |
| `cargo check --locked -p clearra-core-executor --target wasm32-unknown-unknown` | 통과, 경고 없음 |
| `cargo fmt --all -- --check`, `git diff --check` | 통과 |

위 suite에서 다음 경계를 실제 소비자와 독립적인 작은 관계 fixture로 확인했다.

- fixed/materialized PC, 빈/점유 Hold, 원본 ordinal·해별 커버리지·대표 replay가 concrete reference와 같다. 적격 dependency 실행의 `total_build_order_nodes`와 `total_reachability_states`는 0이다.
- 정확한 비영 Build 최종 보드와 두 줄 동시 삭제 뒤 상단 셀의 낙하를 보존한다. 기반 PC가 적격인 finesse Build도 concrete 경로를 사용하며 실제 입력 비용 report와 witness가 reference와 같다.
- CountAll과 명시적 memory cap은 concrete producer에 남는다. sparse ordinal, 중복 큐, 0 가중치, 비영 cursor, terminal release와 서로 다른 arena의 scratch 재사용을 검산한다.
- 동일 exact relation의 suffix 재사용은 두 번째 컴파일 작업을 생략하고, relation 불일치·arena 교체/reset을 거절한다. 서로 다른 물리 lock, 같은 footprint의 orientation alias, eager shard import와 늦은 reveal 반례를 구분한다.
- 원본 큐와 독립적인 snapshot은 전체 킥 테이블 identity를 요구한다. PC의 좁은 fixed/no-hold 예외와 동일한 프로필 이름을 이 별도 evidence 권위로 사용하지 않는다.
- existing fused/reference graph, richer evidence, imported ordered-table serial/parallel PC, final observation reporting 등 전체 코어 회귀도 함께 통과했다. ignored로 표시된 별도 대형 qualification은 이 실행의 통과 수에 넣지 않았다.

최종 코어 감독 receipt의 run ID는 `1795318028257167584-5`, 종료 코드는 0이다. compile을 포함한 감독 시간은 191.212초, process-group RSS의 **샘플 최대**는 2,015,469,568 bytes(약 1.88GiB)였다. memory pressure, timeout, 자동 retry, 종료 시 남은 descendant는 모두 없었다. 이는 unit suite의 자원 기록이며 P7P4 탐색 성능 수치가 아니다.

raw 로그와 receipt는 `_local/artifacts/dependency-consumers-20261122/`의 `core-regression-final.log`, `core-regression-final.runtime.json`, `contracts-kicks-tests.log`, `check-*-final.log`, `format-final.log`에 남겼다. 이 문서는 검토한 요약만 버전 관리한다. 본 변경만으로 전체 see-7 탐색 완료, 모든 보드 전수검사, 측정된 성능 향상 또는 모든 BuildUp 소비자 제거를 주장하지 않는다.

## 10. 다음 전환의 완료 조건

현재 Boolean 지원 범위에서 graph 생성을 지운 것과 모든 concrete 증거 소비자를 없애는 것은 다른 작업이다. CountAll·finesse·spin/score는 선행 Boolean 증명의 대상 밖이므로, 같은 언어라는 이유로 삭제 범위에 넣지 않는다.

1. **예산이 있는 compact 요청과 imported Build:** dependency/compiler·물리 관측·제출 profile의 모든 retained owner와 임시 공존 상한을 admission에 연결한 뒤 지원 조건을 넓힌다. post-evaluation metric 확인만으로 사전 메모리 상한을 대체하지 않는다.
2. **Setup의 공통 Boolean 내부 연산:** 고정 타일링·정확한 완료 조건에 해당하는 부분을 찾아 같은 관계를 재사용하되, partial/residue/terminal supply와 joint 결과의 의미를 별도로 보존한다.
3. **extended 소비자:** Board256 행 frame과 물리 action key를 마련하고 full-height kind-only 관측 union을 교체한다. compact 증명을 그대로 전체 높이 구현 완료로 표시하지 않는다.
4. **대규모 see-7의 구조적 완주:** 동일 suffix·물리 history의 잔여 저장 중복과 relation/policy memo 상한을 분석한 뒤 8미노의 실제 미관측 분기와 10~11미노의 기본 workload를 다시 실행한다. 모든 필요한 draw가 알려진 경우는 oracle 동치를 사용할 수 있다. 과거 전체 P7P4의 relation-limit 실패는 새 실행으로 해소를 확인하기 전까지 미완료로 남긴다.

원격 CI와 제품 승격은 정확한 게시 SHA에 대한 별도 증거다. 로컬 검증만으로 `main` 승격이나 배포 완료를 선언하지 않는다.

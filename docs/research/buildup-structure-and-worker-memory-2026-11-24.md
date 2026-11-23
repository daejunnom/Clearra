# BuildUp의 남은 구조와 worker 메모리 검토

검토 기준: `671584db1d1c5b023ee1d82fbbf72421654c4d0e`.
초안 상태 기록: 2026-11-23 17:42 UTC. 알고리즘·소유권의 읽기 전용 분석과 그 뒤에
작성한 좁은 기존-root 포화 검사 후보를 구분한다. 새 환경의 legacy/current P7P4
실측 결과는 이 시점에 아직 없다.
실제 측정의 source·binary·환경·완료 여부는 별도 실행 보고서로 남긴다.

후속 검증에서는 서로 다른 135개 검사의 성공을 확인했다. Legacy P7P4는 완료했지만
셀 기준선과 변경 후보는 모두 rank 4의 기하 교차에서 signal 9로 종료했다.
변경 후보를 전체 성능 개선으로 판정하지 않는다. 최신 수치와 비교 한계는
[P7P4 실측 보고서](p7p4-comparison-and-memory-2026-11-24.md)를 따른다.

## 1. 작업 경계와 핵심 판단

이전 scratch 경로가 새 환경에서 발견되지 않아 승인된 원격 기준에서 workspace를
복구했다. 복구한 소스는 `/workspace/scratch/0de33b7579b7/Clearra`이며 위 commit을
기준으로 한다. **이번 Mosk 정리는 사용자가 명시적으로 요청한 작업이다.** 이전
환경 감사에서 다른 프로젝트를 보존한 결정과 이번 사용자 요청을 혼동하지 않는다.
이 문서의 알고리즘 감사 자체는 정리·삭제·업로드·build·test·탐색·CI를 수행하지 않았다.

현재 symbolic 경로는 후보별 Build 그래프를 만들지 않지만, 배치 가능한 부분집합을
한 미노씩 늘리는 **BuildUp의 순서 존재 계산은 수행한다.** `G(U,M)`은 geometry Down
판정의 중복을 줄일 수 있으나 이 계산 자체를 제거하지 않는다. 정적 증명으로 끝난
조건은 생략할 수 있고, 나머지는 정확한 의미를 유지해야 한다.

완성 geometry DAG와 공급·이동 조건을 직접 existential relation으로 처리하는 대안을
§5에 제안한다. 이는 rank별 partial family와 반복 Down를 없애는 구현 구조이지만,
보조변수 제거의 폭으로 비용이 옮겨갈 수 있다. **BuildUp과 동등한 병목을 다른 곳에
옮기지 않는다는 보장이나 성능 개선을 달성한 구현으로 주장하지 않는다.**

## 2. 현재 경로에서 BuildUp에 해당하는 계산

[`symbolic_family.rs::solve_with_occupancy`](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family.rs)의
live relation은 selected row 집합과 원래 source의 joint ZDD이며, 공급 상태는 외부
key로 둔다. `for rank in 0..target_depth`는 다음 과정을 반복한다.

1. `current`에서 row 집합을 투영하고 movement domain을 만든다.
2. 기존 source/Hold 전이로 미노별 `ready`와 `ready_rows`를 구한다.
3. 각 row의 `Legal_r`를 원래 row family에 역필터한다.
4. row를 추가한 뒤 `Down(F)`와 교차한다.
5. 허용된 row 집합을 원래 공급 상태/source에 semijoin하고 `next`에 합친다.

Cell mode는 같은 미노의 extension들을 합쳐 Down를 한 번 적용한 뒤 각 extension과
교차한다. 이 분배는 정확하지만, 모든 rank의 부분 배치 관계를 만든다는 사실은
바뀌지 않는다. 실행 순서 목록을 직접 열거하지 않는다는 것과 그 순서가 존재하는지를
집합 전파로 계산하지 않는다는 것은 다른 주장이다.

Legacy 기본 경로도 이름만으로 구분하면 안 된다.
[`buildup/placement_dependency.rs::verify_product`](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup/placement_dependency.rs)는
[`SymbolicPrefixOrders`](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_prefix_orders.rs)의
executed-subset bit planes를 만들고 closed-prefix language를 계산한다.
`BuildOrderGraph` 객체가 없더라도 배치 부분집합의 실행 가능성을 구하는 비용이 남는다.

현재 경로를 비교할 때는 아래 비용을 별도로 봐야 한다.

| 계산 | 현재 표현 | 제거됐다고 볼 수 없는 이유 |
| --- | --- | --- |
| 한 미노의 물리 이동 | cell 또는 row predicate의 pose propagation | 각 보드 assignment의 이동 존재 조건을 계산함 |
| 배치 순서의 존재 | rank별 joint ZDD 전파 | 실행 가능한 부분집합과 source 관계를 생성함 |
| 완성 geometry로 연장 가능성 | immutable family와 partial ZDD의 Down product | 완성 후보 열거는 없지만 product state가 증가할 수 있음 |
| 원래 큐와 Hold | 기존 공급 automaton과 exact semijoin | 같은 multiset만으로 대신할 수 없음 |
| 결과 identity와 coverage | accepted joint relation의 projection | 순서 witness와 구분하여 세어야 함 |

## 3. `G(U,M)`이 해결하는 범위

U는 원래 좌표의 누적 점유이고 M은 **배치한** 미노별 수량이다. 초기 Hold나 읽었지만
배치하지 않은 draw는 M에 넣지 않는다. Full catalog와 허용 최종 수량 집합의 모든
exact cover를 포함하는 producer에서는 같은 U·M을 가진 부분집합이 동일하게 연장
가능하다. 이 경우 geometry 판정에서 row 분해의 중복을 합칠 수 있다.

그러나 `G`를 현재 rank loop에 넣어도 `Legal_r`, source/Hold, add/merge와 원래
identity 보존은 남는다. `G` 자신도 family node×수량 slice, cell-BDD 및 Product의
수량 convolution으로 커질 수 있다. 따라서 이 변경은 **Down 구조 개선**으로
평가해야 하며, BuildUp 제거라고 이름 붙이지 않는다.

또한 모든 family가 이 축약을 허용하지 않는다. 빈 4×4에서 수평 I 네 개의 cover
하나만 가진 F와 수직 I 네 개의 선택은 같은 U·M이지만 후자는 Down(F)에 없다.
[`certify_exact_cover`](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_compatibility_filter.rs)는
region과 비겹침을 인증할 뿐 F의 전체성은 인증하지 않는다. Full producer의 권한,
region-local 연산과 수량 convolution은
[`symbolic-occupancy-quotient-design-2026-11-23.md` §7](symbolic-occupancy-quotient-design-2026-11-23.md#7-후속-설계-점유와-미노-수량으로-geometry-연장-가능성-판정)에
정리되어 있다. 아직 구현한 oracle이 아니다.

## 4. 정적 certificate와 값싼 기존 증명 재사용

### 현재 실제로 생략하는 경로

[`placement_poset::compile_rigid_columns`](../../crates/clearra-core-executor/src/backend/wasm_cpu/placement_poset.rs)는
모든 실제 배치 순서가 만족하는 predecessor 관계를 구하고, 그 poset의 모든 ideal
전이가 실제 harddrop으로 가능함을 인증한다. 인증에 성공하면
[`compile_poset_language`](../../crates/clearra-core-executor/src/backend/wasm_cpu/piece_order_language.rs)와
일반 `cover_piece_language`로 연결한다. 물리 subset projection, BuildOrderGraph와
movement BFS를 만들지 않는다. 같은 종류이면서 전체 predecessor·successor가 같은
작업은 count 좌표로 합치지만, 다른 동종 배치를 임의로 합치지 않는다.

이 certificate의 frame-zero rigid 조건, support 필요성, 최소 보드의 floor 연결과
최대 보드의 빈 corridor 조건은 그대로 유지해야 한다. 실패는 Unknown이며
불가능의 증명이 아니다. 더 넓은 `compile_with_movement_guard`는 삭제 frame별
minimum/maximum과 ordered-kick interval 증명을 이미 제공하지만, 이를 모든 후보에
실패하기 쉬운 전수 prepass로 추가하면 비용이 앞 단계로 옮겨갈 수 있다.

정적 poset가 모든 경우를 표현하지는 않는다. Grounding의 지원자가 `a OR b`일 수
있으며, ordered kick은 선택 target의 비어 있음과 **앞 target들의 충돌**을 동시에
요구한다. 지원자 하나를 임의로 고르거나 OR를 AND로 바꾸면 해를 잃는다. 서로 다른
geometry component도 줄 삭제와 공급 순서로 연결될 수 있다.

### 새로운 subset 순회 없이 활용할 수 있는 증명 경계

기존 `seed_vertical_closure`는 실제 sky seed에서 하강한 도달 family를 계산한다.
한 row r에 대해 이 양성 증명으로 확인한 legality를 P_r, availability·frame·
collision·grounding 같은 정확한 필요조건을 N_r라고 하면 다음 관계가 성립해야 한다.

\[
P_r\subseteq Legal_r\subseteq N_r.
\]

**P_r=N_r가 이미 계산된 동일 owner의 canonical predicate로 확인되면** 그 row의
남은 이동 판정은 생략할 수 있다. 모든 요청 row가 이렇게 포화됐으면 Boolean
물리 계약에서는 나머지 propagation도 필요 없다. 미해결 row가 있으면 정확한
propagation을 유지한다. 이것은 source·미노 이름·kick profile에 따른 예외가 아니다.

현재 코드는 vertical closure를 propagation의 seed로 사용하지만, 위의 모든 row
포화 조건으로 piece 계산을 종료하는 경로는 구현하지 않았다. N_r를 새로 크게
만들거나 모든 subset·frame을 추가 순회하면 이 제안도 값싼 재사용이 아니다.
우선 검토 범위는 **이미 보유한 root의 동일성 또는 기존 interval 증명으로 끝나는
경우**다. 최종 lock의 grounding과 D별 realization까지 포함하지 않은 pose 도달만으로
포화를 선언하면 안 된다. 미해결 target으로 가는 경로에 필요한 pose나 first-success
판정용 이전 kick 후보도 제거하지 않는다.

이번 소스 후보에는 더 좁은 기존-root 조건 `reached[target] == domain`을 추가했다.
모든 source delta가 domain 안에 있으므로 이 target에는 새 assignment가 들어갈 수
없다. 별도 필요조건 BDD 없이 기존 ID를 비교하여 novelty의 NOT/AND와 guard 작업을
생략한다. Collision `free[target]`는 domain 밖에서도 TRUE일 수 있으므로 이를
domain과 혼동하지 않는다. 뒤의 ordered edge는 원본 prior 목록을 계속 읽는다.
Proper-subset delta·skipped prior·다른 rotation slot과 domain 밖 assignment를 검산하는
유한 회귀 소스를 추가했으며, 이 문서 작성자는 build/test를 실행하지 않았다.
이는 propagation의 중복 연산 생략이며 BuildUp 제거가 아니다.

Down의 개별 overlap guard에도 작은 정확한 생략을 추가했다. Row r의 cutoff를
`1 + max{s ≠ r: C_s ∩ C_r ≠ ∅}`로 두고 충돌 row가 없으면 0으로 둔다. ZDD 변수는
low/high 양쪽에서 증가하므로 현재 변수가 cutoff 이상이면 남은 suffix를 통째로
돌려줄 수 있다. 이 반환은 자기 row와 원래 source의 상관관계를 그대로 보존한다.
64개 cell별 가장 큰 두 row ID로 cutoff를 계산하므로 tetromino catalog에서 O(4R)
작업과 row당 u32 하나만 추가한다. 새 predicate나 subset 순회는 만들지 않는다.
이 작은 Vec의 capacity는 retained payload 추정에 포함한다.

Cutoff의 유한 회귀는 pairwise 정의·같은 footprint의 다른 ID·bit 63·충돌 없음과,
기존 전체 guard 순회를 재현한 경로와의 동일 결과 및 감소한 논리 work를 확인하도록
작성했다. 실제 test 실행은 root가 담당한다. 기존 저빈도 storage pulse에서만
`symbolic_compatibility_`의 filter/guard misses, `guard_suffix_elisions`, saturated
unions와 Union remainder calls/created nodes/work를 u64로 보고한다. Hot 연산마다
시계·I/O·새 lock을 추가하지 않는다. 이 생략 역시 Down 전체 폭이나 BuildUp을
제거했다는 뜻은 아니며, 실제 감소량은 새 실측의 별도 판단 대상이다.

기존 진단 경로의 kick interval ledger는 piece·realization·삭제 frame·ordered witness가 동일하고
minimum⊆board⊆maximum일 때 양성 증명을 재사용한다. 이러한 완전한 증명 단위는
재사용할 수 있지만, 다른 board에서 우연히 성공한 결과나 단순한 reachability의
단조성을 가정하면 안 된다. 새로운 solver나 certificate 확장은 이번 문서 작업에서
구현하지 않았다.

## 5. 미구현 대안: 완성 관계의 직접 existential projection

현재 rank별 forward BuildUp와 Down를 실제로 제거하는 구조로 다음 관계를 검토한다.

\[
Accepted(X,q)=Source(q)\land\exists A\;
[Geometry_F(X)\land ExactlyOnce(X,A)\land Supply(q,A)\land Movement(X,A)].
\]

X는 최종 row identity 집합, q는 숨겨진 tail까지 포함한 원래 queue, A는 배치 slot·
순서·Hold/draw 상태·이동 증명의 보조변수다. 완성 geometry DAG를 제약회로로 유지하고
모든 보조변수를 제거한 `(X,q)`만 결과로 남긴다. 이 방식에는 `current/next` partial
family나 rank마다 Down를 적용하는 과정이 없다. **정확한 순서 존재 의미 자체를
없애는 것은 아니며, 그 조건을 다른 연산으로 푸는 것이다.**

### Geometry DAG를 복제·열거하지 않는 최종 조건

[`GeometryCompatibilityFilter`](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_compatibility_filter.rs)의
guard 회로를 재사용할 수 있다.

\[
G_r(X)=\bigwedge_{s\ne r,\;C_s\cap C_r\ne\varnothing}\neg x_s.
\]

Append는 `G_r AND child`, Union은 OR, Product는 AND로 읽는다. 이는 구조 인증된
원래 F의 Down membership이다. 모든 cover가 N개 row이고 선택한 X도 정확히 N개이면
Down(F) membership과 F membership이 같다. 따라서 최종 제약에 이 회로를 한 번
포함하면 된다. 부분 ZDD에 이를 반복 적용하지 않으며, 원래 F를 사용하므로 §3의
full-producer swap-closure 권한도 추가로 필요하지 않다.

### 순서·ILC·이동·공급의 정확한 연결

각 selected row는 N개 slot 중 정확히 한 곳에 나타나고, unselected row는 나타나지
않아야 한다. 같은 미노 종류의 다른 placement도 별개의 row다. slot t의 점유는
독립 witness가 아니라 앞 slot들에서 정의한다.

\[
U_t=I\cup\bigcup_{j<t}C_{r_j}.
\]

삭제 D_t와 물리 B_t는 U_t에서 계산한다. pending row가 이미 삭제된 원래 줄을
차지하는 realization은 기존 catalog 계약대로 거부한다. B만 같고 D가 다른 상태를
합치면 안 된다.

고정 U_t의 유한 pose graph에서는 target에 이르는 이동 증명을 predecessor와
엄격히 감소하는 비음수 pose rank로 표현할 수 있다. 선택된 non-seed pose마다 유효한
guarded predecessor를 요구하고 rank를 감소시키면, 순환 경로끼리 서로 reachable로
인증하지 못한다. 유한 pose 수의 rank 범위는 단순 경로를 증명하기에 충분하다.
Seed/sky authority, target grounding, ordered kick의 모든 이전 실패 조건은 그대로
유지한다. 이는 정확한 회로 인코딩 제안이며 현재 동작하는 새 solver가 아니다.

공급은 기존 exact automaton의 초기 Hold·draw cursor·terminal release·EOF 의미를
재사용해야 한다. 원래 q의 사용하지 않은 tail도 보존한다. 모든 schedule·path·Hold
witness는 **존재 양화로 제거한 뒤** `(X,q)`를 센다. 그렇지 않으면 같은 해법의
여러 실행 순서를 중복 계산한다. 현재 Boolean/CountUnique 범위를 넘어 spin·score·
trace를 지원하려면 last-action·kick 등 추가 상태와 witness 출력 계약이 필요하다.

### 병목 이동을 검증해야 하는 이유

정적 factor graph 또는 decision circuit의 bucket elimination으로 이 관계를 계산할
수 있지만, 중간 factor의 변수 폭은 작다고 보장할 수 없다. 줄 삭제와 source 순서가
geometry Product의 자식들을 다시 연결한다. 각 자식에서 보조변수를 독립적으로
없애거나, 해당 변수를 사용하는 factor를 남겨 두고 일찍 양화하면 결과가 바뀐다.

SAT 모델이나 schedule을 하나씩 뽑아 결과를 합치는 구현은 목표와 맞지 않는다.
기존 rank DP를 새 이름의 evaluator 안으로 옮기는 것도 구조 개선의 증거가 아니다.
제약회로 구성이 작다는 사실만으로 accepted relation의 projection도 작다고 결론
내리지 않는다. 자원 거절·취소는 빈 결과나 부분 성공으로 바꾸지 않는다.

### 다음에 검토할 국소 연산 결합 — 미구현

새 대형 predicate를 만들지 않고 현재 compatibility 연산에서 제거할 수 있는
중간 계산도 두 가지 확인했다. 이번 측정 후보에는 포함하지 않았다.

첫째, 인증된 연속 Append chain의 rows `r₁…rₖ`는 서로 겹치지 않는다. 그러므로
각 선택 row s에 대해 다음 동치가 성립한다.

\[
\bigwedge_i G_{r_i}(S)
\iff
\forall s\in S:\quad
 s\in\{r_1,\ldots,r_k\}\lor
 \left(C_s\cap\bigcup_i C_{r_i}=\varnothing\right).
\]

현재의 `J→J₁→…→Jₖ`를 작은 union cell mask와 원래 row ID 목록을 사용하는
한 restriction으로 합칠 수 있다. 같은 chain 안의 row들은 다른 chain row와
충돌하지 않으므로 자기 ID 예외를 정확히 합칠 수 있다. 원래 source 변수는 그대로
보존한다. 64-bit region과 nonempty·서로소 row라는 일반 계약에서는 chain 길이도
64 이하이며, Full6 tetromino에서는 15 이하이다. Singleton guard memo의 폭넓은
재사용을 잃을 수 있으므로 실제 chain 분포·memo 재사용·중간 node 감소를 함께
계측해야 한다. 이 결합은 순서 존재 계산 전체를 제거한다는 주장이 아니다.

둘째, Union의 왼쪽 결과 L을 구한 뒤 오른쪽의 `(right, original J)` 결과 R이
동일 arena/epoch의 기존 memo에 있으면 `L∪R`를 바로 사용할 수 있다. 이때
`J−L`와 그 residual에 대한 별도 geometry traversal은 필요 없다. 새 결과를 먼저
계산하는 prepass가 아니라 이미 있는 정확한 결과를 사용하는 조건이다. Cache miss
때 늘어나는 조회 비용과 실제 hit 비율을 함께 확인해야 한다.

## 6. Worker 수와 실제 동시 메모리

[`symbolic_family_parallel.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_parallel.rs)는
공통 predicate prefix를 한 `Arc` storage로 공유한다. 공통 BDD·geometry DAG 전체가
worker마다 복제되는 구조는 아니다. 각 piece 작업의 private suffix는 별개다.
`sync_channel(0)`에서도 send에 막힌 worker는 완성한 suffix를 소유하며, consumer도
현재 처리 중인 owner를 보유한다.

따라서 동시 생존 메모리는 다음과 같이 구분해야 한다.

\[
M_{shared}+M_{joint}+M_{Down}+\sum_{live\ pieces}M_{private}+M_{temporary}.
\]

요청 workers가 9여도 이 단계의 task는 최대 7종류다. piece마다 graph 크기가 다르므로
정확히 같은 상수의 worker-linear 메모리라고 부르지 않는다. Worker 수를 줄이는
것과 의미가 끝난 owner의 수명을 줄이는 것도 다른 변경이다. 준비된 worker의 즉시
작업 시작 정책을 유지하며, 공유 graph와 task별 storage를 따로 계측해야 한다.

이전 환경의 piece-batch 실행은 다른 여섯 piece가 released이고 T만 consuming인
상태에서도 큰 coordinator ZDD와 compatibility memo를 남겼다. 이 **과거 미완료
실행**은 [`symbolic-cell-occupancy-2026-11-23.md` §9](symbolic-cell-occupancy-2026-11-23.md#9-미노별-기하-검사-묶음--후속-후보)에
기록되어 있다. 새 환경의 성공 시간·현재 peak나 worker scaling 결과로 재사용하지
않는다. Worker owner 조기 해제는 검토할 가치가 있지만 partial relation×Down의
증가까지 해결한다고 주장할 수 없다.

## 7. 채택 전에 필요한 비용·정확성 증거

| 대상 | 별도로 확인할 항목 |
| --- | --- |
| 기존 baseline와 변경 후보 | 같은 compiled request/source, worker 정책, release 최적화, 완료 여부 |
| 정적 증명 | 인증 비용·실제 생략한 row/constraint 수·Unknown 비율 |
| 직접 회로 구성 | 원본 DAG 공유, 생성한 guard/factor와 보조변수 수 |
| 보조변수 제거 | 최대 factor support·intermediate node·apply work·임시 peak |
| 결과 | 원래 identity×source 관계; global coverage/count만으로 대신하지 않음 |
| 소유권 | live piece 수, shared/private/consumer storage 및 실제 RSS의 동시 관측 |

서로 다른 진행 위치에서 종료된 두 시간을 나눠 speedup을 계산하지 않는다. 조기
release는 결과·오류·취소 경로에서 살아 있는 root의 마지막 사용 뒤에만 수행한다.
해제 전 보고할 work·retained 수치를 확보하며, 결과만 준비됐다는 이유로 반환 ZDD나
공유 prefix가 참조하는 소유자를 버리지 않는다. 새 baseline의 측정 전에는 어떤
후속 설계도 P7P4 완료 시간이나 RSS를 개선했다고 기록하지 않는다.

### P0 — 이번 비교와 수명 변경

- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_parallel.rs`, 구현 내용: 마지막 Boolean 조회 뒤 private owner를 조기 해제하고, scalar accounting·오류·취소·drain·원래 relation 및 Released-before-Down를 유한 회귀로 검증했다. 소스는 `5c70a989b575ce45892070dc565f68ba39597cf3`에 고정했다.
- [x] 수정 파일: `crates/clearra-core-executor/examples/symbolic_family_compare.rs`, 구현 내용: 동일 바이너리의 legacy/current 실행에서 실제 compiled request/source 일치, 완료·미완료, 커널 최대 RSS·표본 RSS·마지막 phase를 구분해 보존했다. 새 ownership 그룹과 compatibility counters는 후속 후보에 포함했다. 결과 범위는 [실측 보고서](p7p4-comparison-and-memory-2026-11-24.md)를 따른다.

### P1 — 이미 계산된 증명으로 조건 제거

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_compatibility_filter.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_compatibility_filter_tests.rs`, 구현 내용: 인증된 연속 Append guard의 결합과 기존 오른쪽 Union 결과 재사용을 검토하고, 사라지는 중간 family·memo 재사용 손실·추가 조회 비용 및 원래 source 관계를 함께 검증한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_family.rs`, 구현 내용: 기존 positive vertical proof와 이미 보유한 필요조건의 동일성으로 row 또는 전체 요청 target의 포화를 판정할 수 있는지 검토하고, 새로운 subset prepass 없이 정확한 unresolved 경계를 유지한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/placement_poset.rs`, 구현 내용: 기존 정적·interval certificate를 사용할 때 필요성과 충분성을 각각 보존하고, source/profile 예외나 전수 실패 prepass 없이 실제 생략 비용을 확인한다.

### P2 — 직접 projection의 별도 연구 계약

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family.rs`, 구현 내용: 분리된 prototype에서 완성 geometry 회로와 정확한 공급·이동 보조변수의 projection을 검토하고, rank-family/반복 Down를 없앤 비용과 elimination width의 새 비용을 함께 계측해 채택 여부를 판단한다.
- [ ] 수정 파일: `docs/research/buildup-structure-and-worker-memory-2026-11-24.md`, 구현 내용: 위 prototype의 실제 결과가 생기면 제거한 비용과 새로 생긴 비용을 함께 기록하고, 폭증이 다른 단계로 옮겨가지 않았다는 주장은 검증 결과가 있을 때만 추가한다.

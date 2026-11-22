# see-7의 추가 몫과 Geometry 가지치기: 적용 조건과 전 경로 증명

작성일: 2026-11-22. 기준 commit은
`c89b330bca8fb6c10e9c6d4752d375999c62b11a`, 작업 branch는
`codex/see7-quotient-pruning-20261122`다. 이 문서는 각 축약의 적용 조건과 모든
허용 경로에 대한 보존 증명을 담는다. 실제 실행·commit·receipt·실패 원인은
[측정 보고서](see7-pruning-measurements-2026-11-22.md)에 구분해 기록한다.

q21까지 실제 8-lock/9-draw see-7의 각 완료 비교에서 원래 211,680개 큐 ID 중
211,234개의 성공 결과가 일치했다. q17의 같은 binary
ABBA에서는 기존 조합의 평균 9.7012302315초가 상계 가지치기 후
3.9884741035초로 줄었다. 이것은 두 표본씩의 기술 통계이며 전체 최적화의 곱셈
속도 개선율이나 모든 입력의 성능 보증이 아니다. q17의 정확성 guard를 유지한
제품 기본 활성화를 후속 코드에 반영하며, 그 외 실험 옵션은 각자 채택 상태를 따른다.

10-lock P7P4는 q18에서 전체 456,459개 후보 등록과 freeze 이후 공통 64M
work 상한에서 미완료다. q18은 같은 graph를 보존하면서 생산·병합·freeze work를
약 50M에서 35,887,800으로 줄였지만 정책 탐색이 남은 예산을 소진했다.
11-lock q18은 아홉 worker가 모두 생산을 마친 뒤 coordinator node 상한에서
중단됐다. 완료된 worker work 합계가 71,396,217이므로 이 producer를 그대로 둔
정책·중앙 node 개선만으로는 기존 64M 안에서 완주할 수 없다(§10).
q21의 큰 exact cone cache는 실제 경로 등록을 줄였지만 cache 성장 비용 때문에
10-lock producer work가 14,437,368에서 18,043,465로 오히려 늘었다.
완료 경계 class도 10-lock의 batch 단계에서 work 한도에 도달했고,
첫 suffix bitmap 구현은 8-lock 정책 607/5,040개만 처리하고 같은 한도에 도달했다.
q22는 이 실측에 따라 중간 T만 exact cache에 보관하고, 경계의 중복 순회·정렬과
target 공급 bitmap의 반복 구성을 줄인다. 구현 및 실행 상태는 측정 보고서를 따른다.

유한 회귀는 구현 실수를 찾는 근거이고 이 문서의 전칭 증명을 대신하지 않는다.
조합론적 상태 수, 완료 시간, 실패까지의 시간, 관측 RSS 및 제품 기본값을 서로
구분하며, 10/11-lock 완주와 최종 exact-SHA CI가 확인되기 전에는 완료로 표시하지 않는다.

## 1. 결론과 네 가지 후보

현재 큰 중복은 완성 물리 관계를 보관하는 `(candidate, prefix)` 원소의 반복과,
관측 trie를 모두 만든 뒤에야 같은 미래를 합치는 데 있다. 기존에 있는
`fully_revealed_node`, 원래 정보집합의 결정적 미래 certificate, 개별 revealed root
membership을 새로운 최적화로 다시 세지 않는다.

| 후보 | 보존하는 것 | 추가로 필요한 조건 | 예상 이득과 비용 | 적용 판단 |
| --- | --- | --- | --- | --- |
| A. Dormant Frontier, live relation 공유와 reveal 하계 | 경계 전 물리 commitment, 경계 후 정확한 종류 잔여 언어, Boolean 정책 값 | 정확한 hidden horizon, 원래 공급의 reveal 권한, 같은 종류 arena. 하계는 인증된 전체 fresh bag에 한정한다. | 경계 배열·재스캔과 불필요한 suffix binding 보유·merge를 줄인다. 더 이른 행동 signature는 계산 비용을 제한한다. | A1/A3/A4와 q4의 board-only root0 binding 생략 및 A5 original-source index 생략을 구현했다. A2의 추가 signature는 미적용이다. |
| B. 완전한 생산 뒤 freeze한 board DAG 또는 공급 상대 정책 memo | 전체 물리 family에서는 raw 언어, 공급 상대 family에서는 원래 Ω와 product한 root의 정확한 정책 값 | 아래의 family 완전성 certificate와 실제 board 전이의 soundness. 단순히 같은 board라는 사실만으로는 부족하다. | candidate/prefix 장기 보유와 반복 scan을 제거할 수 있다. 모든 필요한 prefix/edge의 등록·merge 비용은 남는다. | B5–B20을 구현하고 실제 8-lock 비교를 진행했다. q17의 exact incumbent 가지치기는 해당 fixture에서 큰 비용 감소를 보여 guard를 유지해 기본 활성화했다. Prefix mask·공개 전 memo·multiset group index 등 비용 개선을 확인하지 못한 선택 기능은 기본 off다. q21의 cone/class 실험도 10-lock을 완주하지 못했으며 전체 생산·병합 비용을 함께 측정한다. |
| C. residual 언어 포함관계와 제한된 multiset bitmap | 같은 residual union의 정확한 종류 언어 | 포함관계 증거 또는 제한된 r·uniform multiset certificate | 중복 root를 줄일 수 있으나 pair 비교나 bitmap materialization 비용이 더 클 수도 있다. | 일반 언어 포함관계·producer 전체 bitmap 교체는 미적용이다. q20/q21의 실제 공개 r≤6 membership bitmap은 8-lock에서 work-limit 실패했다. q22는 target M의 공급 재명명, frozen board OR와 Full/Empty certificate로 같은 교집합의 계산을 공유한다. 기본 off 및 실제 계측에 따른 채택 원칙을 유지한다. |
| D. 공급 미래를 먼저 factor하는 observation source | 관측별 원래 ID·질량·공급 전이·EOF | 정확한 source 구조와 ordinal 대응. 처음에는 완전 canonical standard bag으로 한정 가능 | 거대한 raw trie의 생성·보유를 줄인다. 성공 bitset의 원래 ID 복원은 남는다. | trie 비용이 실제로 지배할 때 후속 구조 개선으로 적용한다. |

Geometry의 `off/current/improved`는 이 네 정책 축약과 별도 축이다. 현재 전역 E2는
같은 정보로 판단하는 단일 edge 제거의 필요충분조건이다. 이번 improved는 같은
판정을 두 번 하지 않도록 공유하는 개선이며, 제거되는 edge 수를 늘리는 개선이
아니다. 정확한 설명과 증명은 §7에 둔다.

### 기존 증명과의 관계

- [고정 타일링 증명](fixed-tiling-ilc-dependency-proofs-2026-11-22.md)의 H1–H4는
  줄 삭제의 rank/select와 실제 보드 대응, P1–P4는 물리 commitment, Q1–Q3는
  미래 관계의 합동을 제공한다. 후보 A는 Q1을 숨은 horizon에 맞게 제한하며,
  후보 B는 **완전 family**라는 추가 양화로 fixed-candidate 제약을 제거한다.
- [관측 몫 증명](observation-quotient-proof-2026-11-22.md)의 정리 1은 공통 이력의
  residual, 정리 4/4A와 6은 reveal 경계, 정리 7은 가중 미래의 재사용을 제공한다.
  후보 A와 D는 그 계산을 더 이른 저장 경계에서 수행한다.
- [보드 family 증명](combinatorial-board-family-proof-2026-11-22.md)의 E2/D1/C3는
  edge 제거·목표별 영향·정확한 치환을 구분한다. source/target 밖의 blocker 읽기를
  지우거나 상관된 보드를 독립 변수라고 가정할 권한은 없다.
- [임의 순서 킥 증명](parametric-kick-equivalence-2026-11-22.md)의 U/R/E2는 실제
  ordered table, first-success, finite all-sky 경계를 보장한다. 이 문서의 정책
  축약은 그 정확한 물리 관계를 입력으로 받으며 새 킥 의미론을 만들지 않는다.

위 문서의 일부 제품 범위 설명은 작성 시점의 standard-bag 전용 admission을
서술한다. 현재 `SearchProblem::pc_placement_dependency_eligible`는 materialized
공급 일반화를 포함한다. 여기서는 유한 원래 ID 집합에 대한 일반 정리와,
standard bag에만 사용할 수 있는 구체적 저장 최적화를 구분한다.

## 2. 공통 상태와 경계

원래 공급 ID 집합을 Ω, 비음수 가중치를 μ, 현재 실제 관측 이력과 양립하는
정보집합을 I라 한다. 같은 단어라도 원래 ID가 다르면 개수와 bitset에서 구분한다.
후보가 성공한 큐만 남긴 집합으로 I를 바꾸지 않는다.

물리 행동은 `a = (piece kind, 현재 네 셀)`이며, hold 선택과 lock은 현재 계약에서
하나의 결정이다. 그 뒤 변경된 cursor에 따라 공급이 공개된다. 전체 lock 깊이는 n,
현재 깊이는 k, 초기 cursor는 c₀, 유효 source 길이는 N, 관측 폭은 w=7이다.
N은 projected lookahead를 포함하며 원래 materialized EOF와 구분한다.

공통 물리 이력 h 뒤의 정확한 concrete residual 집합을 R(h)라 하자. 원소는 현재
구현의 `(candidate, executed prefix)`다. 같은 행동별 derivative는

\[
R(ha)=\{s':s\in R(h),\ s\xrightarrow{a}s'\}.
\]

후보의 operation 이름이 달라도 같은 실제 행동을 수행한 뒤에는 이 집합으로
합친다. 반대로 같은 종류라는 이유만으로 서로 다른 네 셀의 행동을 합치지 않는다.

`Current`와 `SwapHeld`는 1 draw, 빈 hold의 `StoreCurrent`는 2 draw를 소비한다.
0 draw인 terminal-held-release는 accepting 자식으로만 간다. 따라서 비종료
깊이 k에서 `cursor ≥ c₀+k`이며 다음 경계는 보수적인 상계다.

\[
H=\min\bigl(n,\max(0,N-c_0-w)\bigr).
\tag{H}
\]

H가 n보다 작으면 깊이 H의 모든 비종료 상태는 전체 유효 source를 관측했다.
H=n이면 해당 경계는 terminal이다. 실제 evaluator는 전체 source 관측 외에도
원래 정보집합의 유일한 suffix와, terminal target 없는 fixed-horizon의 관련 draw
certificate로 더 일찍 종류 언어를 사용할 수 있다. H는 그 권한을 늦추는 상계다.

각 정확한 residual s의 완성 종류 언어를 K(s)라 쓴다. 같은 pinned
`PieceOrderLanguageCache`의 suffix root는 그 언어에 대한 증거다. 숫자 ID가 다른
arena에서 같다는 사실은 아무런 동치를 뜻하지 않는다.

## 3. 후보 A: 공개 경계에서 필요한 자료만 보관

### A1. Dormant Frontier 정리

깊이 H−1에서 실제 행동 a를 먼저 구별해 derivative를 만든 뒤,

\[
F(ha)=\operatorname{unique}\{K(s):s\in R(ha)\}
\tag{F}
\]

만 저장하자. 이 자료는 아직 실행 가능한 종류 언어가 아닌 `Frontier`다. evaluator가
원래 정보집합의 reveal certificate를 확인했을 때만 그 합집합을 활성화한다.

**정리 A1.** 이 저장 변경은 모든 지원 공급 ID에서 가능한 관측 정책의 성공과
실패를 보존한다. 가중 성공 질량과 최적 Boolean 정책 값도 같다.

**증명.** H 이전에는 원래 `(kind,cells)` derivative를 그대로 사용하므로 공통
물리 이력과 양립하는 후보가 정확히 유지된다. 마지막 행동 a도 공개 전에 동일한
물리 label로 선택한다. 경계 후 원래 evaluator가 미래 확정을 증명하면, 특정 실제
공급과 hold에 compatible한 물리 완성 실행이 존재할 필요충분조건은 F(ha) 중
어느 한 정확한 종류 언어에 compatible한 단어가 존재하는 것이다. 이는 종류
투영의 정의와 관측 몫 정리 4/4A다. 같은 root의 중복 삭제는 존재적 합집합을
바꾸지 않는다. 성공한 종류 단어는 그 root를 제공한 원래 residual의 물리 witness로
되돌릴 수 있다. 이 선택은 이미 확정된 관련 공급에만 의존한다.

경계에서 양방향 성공 대응을 얻었으므로 더 이른 깊이의 각 물리·공급 행동과
관측별 합, 행동의 최댓값이 귀납적으로 같다. Frontier가 인증 전에 전개를 거부하면
아직 모르는 미래를 이용할 추가 행동은 생기지 않는다. H=0에도 같은 경계 논증이
적용되며, H=n의 accepting 상태는 원래 terminal 조건을 유지한다. □

`Sources::Frontier`와 `Sources::Revealed`는 interning/memo에서 서로 다른 variant여야
한다. 아직 인증되지 않은 Frontier의 `revealed_alternatives`는 없고,
비종료 Frontier의 `prepare_edges`는 명시적으로 거부한다. 이 거부를 빈 언어의
성공적인 계산 결과로 바꾸어서는 안 된다.

작업 중 구현을 읽은 결과
[`queue_observation_factored_actions.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_factored_actions.rs)는
마지막 derivative를 `(cells, child suffix root)`로 정렬·중복 제거한 뒤 같은 cells끼리
Frontier를 만든다. `fully_revealed_node`만 별도 Revealed node로 전환하며,
`remaining_depth=0`의 accepting 의미를 유지한다. 32개 중복 후보의 경계 root가
1개로 줄어드는 회귀와 H=0 회귀가 추가되어 있으나 이 문서 작성자는 실행하지 않았다.

기존 경계의 concrete 원소 수를 m, 서로 다른 종류 root 수를 u라 하면, retained
목록은 m개의 `(candidate,prefix)`에서 u개의 root로 바뀐다. 원래 소유자에 후보와
proof가 남아 있으므로 전체 후보 저장이 없어지는 것은 아니다. 또한 최초 physical
derivative의 후보 순회는 남는다. 직접적인 절약은 경계 child의 큰 배열, 그것을
다시 순회해 root로 바꾸는 작업, 같은 root 집합의 반복 보유다.

### A2. 제한 깊이 행동 합동

한 단계 더 나아가, 깊이 H의 node에는 정확한 K를 놓고 그보다 이른 node에는
정렬된 `(kind,cells,child class)` 전체 목록을 놓는 bottom-up signature를 정의한다.
remaining depth, accepting 조건과 같은 arena를 signature domain에 포함한다.
같은 행동에서 여러 child를 허용하면 그 child들의 정확한 합집합도 보존한다.

**정리 A2.** 같은 signature를 가진 residual은 모든 지원 source/hold 관측 상태에서
정책 동치다. H에서 A1로 동치이고, 한 단계 앞에서는 선택 가능한 실제 행동과
대응한 child class가 같다. 동일 종류는 같은 공급 전이와 reveal 분할을 만들므로
후속 값의 합과 최댓값이 같다. H−k에 대한 양방향 귀납으로 끝난다. □

이것은 고정 타일링 증명의 Q1에 경계 언어 K를 붙인 유한 깊이 특수 경우다.
정확한 구조 equality는 충분조건이다. 다른 signature가 서로 다른 의미라는 역방향
주장이나 최소 automaton을 만들었다는 주장은 필요 없다.

전 후보의 모든 `2^n` prefix에 이 signature를 선계산하면 최적화 비용을 앞당긴
것뿐일 수 있다. 우선 마지막 숨은 layer, 이미 요청된 derivative, 작은 memo 예산에
한정한다. **현재 요청한 한 piece의 edge만 같다는 사실로 node 전체를 합치지 않는다.**
나중의 다른 관측/hold가 다른 piece를 요청할 수 있다. 부분 결과 재사용은
`(원래 node, piece)` 범위에 두거나, 모든 가능한 행동의 equality를 먼저 증명한다.

소규모 보조 개선으로 R을 `(relation,prefix) → candidate ID 집합`으로 lossless하게
묶고, `live_operations_after`를 그룹마다 한 번 계산할 수 있다. 같은 candidate에서
같은 deleted-row mask의 frame도 재사용할 수 있다. 그러나 candidate 집합과 prefix
집합의 독립 곱을 만들면 안 된다. 예컨대 `{(A,S),(B,T)}`를 `{A,B}×{S,T}`로
바꾸면 두 개의 거짓 상태를 추가한다. dense bitmap은 실제 밀집한 그룹에서만 쓰고,
원소·배열 capacity와 캐시 수명은 기존 retained/work 한계에 포함해야 한다.

### A3. 완성 경로가 읽는 labelled relation만 정규화

`ExactPrefixRelation`의 permitted-parent 행을 Pᵢ(S), active-prefix 집합을 A라 하자.
현재 suffix compiler는 active parent만 순회하고 active child의 class만 edge에 넣는다.
physical derivative도 active root에서 시작해 아직 실행하지 않은 i와 active child만
방문한다. 따라서 다음 행은 그 소비자들이 실제로 읽는 전체 labelled edge와 같다.

\[
\widehat P_i(S)=P_i(S)\land A(S)\land\neg S_i\land A(S\cup\{i\}).
\tag{L}
\]

pieces의 operation 순서, n, A 전체와 모든 정규화 행이 같으면 두 관계의 모든 active
prefix에서 동일 operation이 허용된다. terminal부터 귀납하면 모든 suffix 종류
언어도 같다. geometry는 각 candidate가 그대로 소유하므로 이 공유는 다른 물리
footprint를 합치지 않는다. 같은 operation label의 허용 여부만 공유한다.

따라서 `CompactPrefixRelation::new`, `matches`, `relation_hash`가 모두 같은 식 L을
사용하면 원본 permitted table의 비생산적 bit 차이 때문에 놓치던 공유를 회복할 수 있다.
원본 compiler가 부여한 physical-action authority와 active 집합은 그대로 둔다.
이 정규화를 전체 local guard가 같다는 주장으로 확대하지 않는다.

64-lane word에서 i<6이면 아직 i가 없는 lane에 한하여 child-active word는
`active[word] >> (1 << i)`다. i≥6이면 아직 해당 word-bit가 없는 word에서
`active[word | (1 << (i-6))]`를 읽는다. 실행된 i의 lane/word를 반드시 제외한다.
`i<n`과 `ceil(2^n/64)` stride가 child index의 범위를 보장한다. hash만 정규화하고
equality는 원본 행과 비교하는 혼합 구현은 공유 효과를 주지 못하며, 그 반대는
올바른 hash/equality 계약을 깨뜨릴 수 있다.

### A4. 원래 전체 fresh bag으로 증명한 최소 reveal 깊이

H는 늦어도 종류 언어로 전환할 수 있는 상계다. 반대로 **아직 전환할 수 없는**
초기 깊이를 원래 공급에서 증명하면, 그 구간의 suffix root binding을 보관할 필요가
없다. 이는 물리 행동을 생략하는 최적화가 아니다. 원래 physical relation과 모든
물리 derivative는 유지하며, 어떤 정상 evaluator 경로도 읽지 않을 보조 binding만
지운다.

추가 전제는 실제 원래 source가 fresh standard-bag 전체 집합이라는 것이다. 현재
구현은 `MaterializedPatternUniverse::uniform_compact_source`의
`Standard7Bag` variant로 **실제 sequence 저장소와 uniform weight 저장소**를
확인한다. `structure` 설명이나 universe ID, 샘플 일치로 권한을 얻지 않는다.
또한 다음 조건을 함께 확인한다.

- `complete`이고 `truncation_reason`이 없다.
- 실제 sequence 길이가 `problem.supply().source_sequence_length()`와 같다.
- 실제 pattern 수가 universe의 pattern 수와 같으며, checked 정수 곱
  \(\prod_{i=0}^{N_{\rm mat}-1}(7-i\bmod7)\) 및 `total_possible_pattern_count`와 같다.
- projected last piece를 더할 때 원래 길이는 6 mod 7이며, 늘어난 유효 길이 N과
  cursor·관측 폭의 합은 overflow 없이 계산한다.

이 증거는 full source의 순서와 모든 원래 ID를 포함한다. 임의 explicit/factorized
공급, 잘린 lexicographic source, 관측 후 좁아진 source는 이 최적화의 권한을
받지 않고 하계를 0으로 둔다. `complete_standard_bag_ordinals`는 별도 compact
codec 범위 때문에 12 draw에서 `None`이지만, 실제 compact sequence 저장소와
위 checked cardinality 증명은 12 draw에도 유효하다. ordinal codec의 범위를
임의로 넓히거나 성공한 candidate의 ID 집합을 참조할 필요가 없다.

전체 fresh bag의 아직 공개되지 않은 suffix가 유일해지는 최초 관측 깊이는

\[
D=N-\mathbf1_{N\bmod7=0}.
\tag{D}
\]

N이 bag 경계라면 마지막 한 조각은 앞의 여섯 종류로 확정된다. N이 bag 중간이라면
마지막 요청 draw를 보기 전에는 적어도 두 종류가 남는다. 더 앞의 prefix에는
그 뒤의 full/partial bag에 적어도 두 완성 suffix가 존재한다. projected last piece는
그 여섯 조각에서 추론한 같은 원래 ID의 결정적 마지막 draw이므로 식 D가 그대로
성립한다. 여기서 미래가 유일하다는 것은 **원래 정보집합 전체**에 대한 말이다.
성공한 후보만 남겼더니 유일해진 미래는 이 권한이 아니다.

초기 hold가 비어 있고 hold가 허용되면 E=1, 그 외에는 E=0이라 하자. 비종료 k-lock
경로의 draw 보존식은 `cursor−c₀=k+h_k−h₀`이며 h는 hold 점유의 0/1 값이다.
따라서 `cursor≤c₀+k+E`다. 0-draw terminal-held-release는 다음 비종료 결정을
만들지 않으며 이 상계도 깨지 않는다. 관측된 prefix 깊이 d는 다음을 만족한다.

\[
d\le\min(N,c_0+k+E+w).
\]

전체 EOF 관측 또는 원래 미래의 유일성으로 reveal하려면 d≥D가 필요하므로
그 두 권한은 `k < (D−c₀−w−E)₊`에서 발생할 수 없다. 나머지 관련 draw certificate는
remaining lock 수 r=n−k와 현재 빈 hold의 추가 draw e에 대해
`d ≥ min(N,cursor+r+e)`를 요구한다. 아직 d<N인 상태에서는 이 조건과
`d≤cursor+w`를 함께 만족하려면 `r+e≤w`, 따라서 `r≤w`여야 한다. 그러므로
`k<(n−w)₊`에서도 그 권한은 발생할 수 없다.

**정리 A4.** 다음 하계 L에 대해 `0≤k<L`인 깊이에는 어떤 원래 공급·관측·hold 경로에서도
`fully_revealed_node`가 승인되지 않는다.

\[
L=\min\bigl((n-w)_+,\;(D-c_0-w-E)_+\bigr).
\tag{R}
\]

**증명.** k<L이면 위 두 하계를 모두 만족하지 못한다. 따라서 전체 EOF,
원래 deterministic future, 관련 draw certificate의 세 승인 경로가 모두 불가능하다.
물리 node 전개는 보조 suffix root를 읽지 않으며 마지막 hidden child가 읽는 깊이 H는
L 이상이다. 해당 binding을 제거해도 모든 실제 호출의 입력과 결과가 그대로다.
이상으로 전체 경로에서 물리 언어·reveal 시점·정책 값과 기존 선택 경로가 같다. □

현재 구현은 `original_standard_bag_reveal_floor`에서 식 R을 계산하고,
`FactoredActionRelations`의 모든 candidate/worker merge에 같은 n,H,L을 요구한다.
L>min(H,n)이면 잘못된 계약으로 거부한다. **Factored owner**는 등록된 관계의
종류 root 재사용에 prefix0 binding을 읽으므로 prefix0와
`L≤popcount(prefix)≤H`를 보관한다. 다른 공급의 하계를 섞는 것은 허용하지 않는다.

**q4 board owner의 root0 생략.** L>0이면 `n>w` 및 `D>c₀+w+E`다. 초기 관측 깊이는
`c₀+w` 이하이므로 root에서도 EOF, 원래 미래 유일성 및 관련 draw 인증이 모두
불가능하다. E를 k=0에서도 쓸 수 있다고 계산한 것은 하계를 약하게 만들 뿐이다.
초기 occupied hold, 확정된 bag 마지막 조각, 0-draw terminal-held-release에도 예외가
없다. [`queue_observation_board_actions.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_board_actions.rs)의
`register_inner`와 `freeze`는 board suffix binding을 `depth≥L`일 때만 요구한다.
physical root와 그 edge는 그대로이며 L=0인 root binding도 남는다.

이 생략은 **canonical 후보 종류 root의 삭제가 아니다.** 후보 buildability는 여전히
`compile_relation_language`가 만든 root와 원래 공급의 product로 판정한다.
`prefix_suffix_scratch[0]`도 그 과정에서 생성된다. Board owner는 factored 관계의
`registered_language` 재사용을 하지 않으므로 dormant root0 binding만 지울 수 있다.
직접 절약은 initial-board suffix root 보유·worker merge·freeze 비용이며, canonical
언어 계산이나 physical prefix 순회를 없앤다는 뜻은 아니다.

최종 evaluator는 q4의 `QueueObservationPolicyEvaluator::for_original_universe`로
전체 원래 ID domain을 그대로 사용한다. 이는 아래 A5의 index 생략이며 subset이나
candidate 성공 ID를 source로 삼는 변경이 아니다.

예를 들어 c₀=0,w=7, 빈 hold 허용일 때 10 lock/11 draw는 H=4,L=3이고,
11 lock/12 draw는 H=5,L=4다. 첫 경우에는 깊이 1–2, 다음 경우에는 1–3의 binding이
불필요하다. active set 전체를 삭제하거나 canonical 종류 compiler의 중간 계산을
없애는 것은 아니다. 절약되는 것은 retained binding 배열, 등록 work charge,
worker의 suffix root 재결합·검증이다. 이 구별은 등록 단계의 work limit과 실제
정책 평가 병목을 따로 해석하는 데 필요하다.

### A5. 전체 원래 universe의 사용하지 않는 position index 생략

[`queue_observation_policy.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy.rs)의
`ObservationPatternDomain::{OriginalUniverse,Indexed}`는 같은 `ObservationTrie::compile`
및 같은 정책 kernel을 공유한다. 전체 universe에서는 global ID가 `0,…,N−1`이므로
그 identity 배열과 position-by-piece bitplanes를 먼저 만들 필요가 없다.

**정리 A5.** 같은 원래 universe 전체를 입력했을 때 `for_original_universe`는
기존 full `PatternPiecePositionIndex` 생성 경로와 trie·원래 ID·가중치·EOF 및 정책
결과를 보존한다.

전체 index는 원래 ID를 오름차순으로 한 번씩 넣고 local→global map은 identity다.
새 경로도 같은 순서로 `write_sequence_at(id)`와 `weight_at(id)`를 호출하고 같은
trie insert를 수행한다. 같은 단어를 가진 서로 다른 ID를 dedup하지 않으므로 가중
중복도 보존된다. 원래 materialized 길이는 **projection 이전 길이의 최댓값**으로
구하고, 확정 bag 조각을 덧붙인 뒤의 uniform-length 검사는 기존과 같다. 그러므로
terminal-held release가 읽는 materialized EOF와 실제 관측 길이도 각각 같다.
삽입·가중 합산 순서가 같으므로 이후 future class와 정책 재귀도 그대로다. □

이 정리는 fresh bag이나 uniform weights만의 최적화가 아니다. 실제 회귀는 weighted
duplicate ID, 동일 word로 projection되는 서로 다른 원래 길이, 초기 cursor/hold 및
EOF를 대조한다. 다만 **subset 소비자는 Indexed를 유지**한다. 비연속 원래 ID
`[1,6]`을 `0,1`로 바꾸거나 전체 universe로 넓힐 권한은 없다.
이 작업은 불필요한 bitplanes를 생략한 것이며 D의 직접 source DAG를 구현한 것이
아니다. 전체 원래 word를 trie에 넣는 비용은 남는다.

## 4. 후보 B: board/depth를 충분한 미래 상태로 만드는 조건

같은 현재 board라는 사실만으로 fixed-candidate residual을 합칠 수는 없다.
하지만 모든 물리 완성 실행을 포함하는 family라면 결론이 달라진다.

### B1. 바닥부터 점유 행이 연속한 초기 보드의 전체 exact-cover family

다음을 모두 가정한다.

1. 실제 초기 보드 B₀는 폭 W, 목표 높이 H_b 안에 있고, 점유된 row index 집합은
   `{0,…,h₀−1}`이다. 빈 보드도 h₀=0으로 포함한다. 목표는 정확히 n lock 후 empty이며
   `|B₀|+4n=W·H_b`다. 기존 초기 full-row 거절 계약은 유지한다.
2. 미노는 표준 tetromino이고 lock은 실제 grounded placement다. 완성된 행은 즉시
   삭제하고 나머지 행을 순서대로 내린다.
3. 이동 규칙, entry 계약과 kick authority는 요청 안에서 고정돼 있다. 미래 가능성에
   영향을 주는 score, 입력 이력, finesse 비용, 색상·특정 타일링 제한이 없다.
4. 원래 H_b행에서 B₀의 빈칸을 채우는 **모든** 허용 ILC exact cover를 family에
   포함하고 각 cover의 전체 정확한 물리 관계를 보존한다. source multiset이나
   성공 큐로 cover를 빼지 않는다.

**보조정리 B1a.** 완성 가능한 모든 prefix에서 `현재 점유 높이 + 이미 삭제한 행 수 ≤ H_b`다.

가정한 초기 보드의 점유 행은 바닥부터 연속한다. 기존 top 높이가 h일 때
새 grounded tetromino는 바닥에 닿거나 기존 셀의 바로 위에 닿는다. 표준 shape가
차지하는 y좌표는 연속 구간이므로 새로운 점유 행은 기존 구간과 겹치거나 바로
이어진다. full row 삭제 후에도 남은 행이 압축되므로 이 불변식이 유지된다.
더 구체적으로 기존 셀과의 ground contact가 y행의 새 셀에서 발생하면 그 아래
y−1행에 기존 셀이 있으므로 y≤h다. 따라서 새 shape의 최저행도 h 이하이고,
기존 점유 구간 `{0,…,h−1}` 위에 완전히 빈 행을 사이에 둔 새 점유 구간을 만들 수
없다. floor contact도 같은 결론을 준다.

현재 점유된 서로 다른 h개 행은 empty가 되려면 앞으로 각각 삭제되어야 한다.
삭제 전에는 점유 셀이 다른 행으로 합쳐지거나 사라지지 않는다. 전체 완성 실행의
삭제 행 수는 면적 보존으로 H_b이므로, 이미 d개를 삭제했다면 `h≤H_b−d`다. 한 lock
직후 삭제가 일어나는 경우에도 삭제 전 높이를 h_pre, 이번 삭제 수를 e라 하면
`h_after=h_pre−e`와 `d_after=d+e`이므로 같은 부등식이 성립한다. □

**보조정리 B1b.** 모든 완성 물리 실행은 원래 H_b행에서 B₀의 빈칸을 덮는 어떤
ILC exact cover로 lift된다.

실제 삭제 이력 D를 따라 각 lock의 네 셀을 rank/select 역상으로 원래 행에 올린다.
B1a에 의해 어떤 셀도 원래 H_b행 밖으로 나가지 않는다. surviving 초기 셀의 역상은
계속 같은 원래 B₀ 셀이다. 물리 충돌이 없었으므로 새 네 셀은 B₀ 및 이전 operation의
원래 셀과 겹치지 않고, 삭제된 원래 행에는 새 셀이 들어가지 않는다. 끝에서 B₀와
`4n`개의 새 셀을 합친 면적이 `W·H_b`이므로 원래 H_b행을 정확히 덮는다. 고정
타일링 H1–H3의 완전한 ILC 열거가 이 skeleton들을 포함하고, 실제 움직임이 각
dependency edge의 witness다. 역방향은 기존 P1의 lift/forget다. □

여기서 **ILC skeleton은 원래 좌표에서 연결된 tetromino일 필요가 없다.** 중간
original row가 먼저 삭제된 뒤에 놓인 O가 그 경계를 걸치는 경우도 포함한다.
예를 들어 삭제 집합 D가 original row 1을 포함하면, current rows `[0,1]`을
차지한 O의 역상은 original rows `[0,2]`에 같은 두 x좌표를 가진 네 셀이다.
이는 원래 좌표에서 두 부분으로 나뉘지만 삭제 후에는 정확한 O다.

이 경우의 구현 연결은 다음과 같다.

- [`catalog.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs)의
  `compile_inverse_clear_realizations`는 표준 종류·회전·x 각각에 대해
  `enumerate_row_projections`를 호출한다. 후자는 occupied local row의 모든 증가
  target-row tuple을 열거하므로 O의 `[0,1]→[0,2]`를 생성한다. 중간 row 1은
  `required_deleted_rows`에 기록한다. `compile_with_clear_row_domain`의 PC 분기는
  `ClearRowDomain::unrestricted()`를 사용하므로 그 gap을 잘라내지 않는다.
- 같은 파일의 `instantiate_realization_raw`는 요구 gap이 D에 들어 있고 footprint가
  D의 행과 겹치지 않는지 확인한다. 이어 실제 shape를 놓은 뒤 각 current row를
  `target_row_for_current_row`로 되올려 원래 네 셀과 **정확히 같은지** 확인한다.
  위 O는 D=`{1}`에서 복원되며, row 1이 아직 남아 있으면 거절된다.
- [`placement_poset.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/placement_poset.rs)의
  `CandidateGeometry::compile_for_final_board`는 original cells의 disjoint exact cover를
  검증한다. `symbolic_deleted_rows`와 `symbolic_frame`은 실제 완료된 contributor
  행들을 원자적으로 투영한다. 여러 행 동시 삭제, 서로 떨어진 행 삭제, 다른 시점의
  연속 삭제를 특정 한 방식으로 제한하지 않는다.
- [`reachability_symbolic_target.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_target.rs)의
  target seed는 현재 shape가 비충돌이며 floor 또는 아래 pose 충돌로 grounded인지
  검사한다. 실제 이동 가능성은 같은 요청의 entry·ordered kick 계약으로 증명한다.

따라서 B1a의 `h+d≤H_b`는 모든 성공 lock의 역상이 유한 catalog 높이 안이라는
누락 방지 근거이고, H1–H3 및 위 함수들은 **벌어진 역상도 catalog가 실제 포함한다**는
근거다. 여기서 물리계 M은 같은 요청의 entry·reachability ceiling·kick authority를
사용한다. 별도의 무한한 이동 높이나 다른 spawn 규칙까지 동치라고 확장하지 않는다.

이 전제는 셀 전체의 연결성이나 full-width 점유를 요구하지 않는다. 각 occupied row가
바닥부터 중간 빈 행 없이 존재하면 된다. 폭 10에서 B₀=`0xff`, H_b=4, n=8은
`8+4·8=40`, B₀=`0x3f`, H_b=5, n=11은 `6+4·11=50`으로 조건을 만족한다.
두 초기 보드 모두 바닥 행 하나만 점유한다. 중간에 완전히 빈 행이 있는 일반
nonempty 보드에는 이 귀납의 기저가 없으므로 별도 증명 없이 확대하지 않는다.

**정리 B1.** 이 full family에서 두 공통 물리 이력 h₁,h₂가 같은 현재 board B와
같은 깊이 k에 도달하면, 두 residual의 완성 **물리** suffix 언어는 같다.

물리 엔진의 미래 continuation 집합을 `L_phys(B,n−k)`라 하자. 각 family continuation은
실제 물리 실행이므로 이 집합에 속한다. 반대로 어떤 `w∈L_phys(B,n−k)`도 h₁w와
h₂w 양쪽에 붙일 수 있다. 고정된 Boolean 이동 규칙은 과거가 아니라 현재 B를 읽는다.
B1b가 각 완성 실행을 full family 안으로 lift하므로 w는 양쪽 residual에 속한다.
양방향 포함관계에서 equality가 나온다. □

이때 서로 다른 원래 deleted-row 집합 D도 별도 key에 남길 필요가 없다. D를
잊어도 된다는 결론은 frame이 무관해서가 아니라, **각 D에 맞는 모든 역상 cover가
family 안에 존재하기 때문**이다. 같은 초기 보드의 면적 보존으로 k와 B가 같으면
삭제 행 수도 같지만, implementation key에 k/remaining horizon을 명시하는 편이 안전하다.

**더 보수적인 대안.** key에 D까지 남기면 전체 물리계의 lift 정리 없이도 좁은
splice closure를 직접 증명할 수 있다. 같은 `(B,D,k)`는 원래 occupied mask
`U = full_rows(D) ∪ lift_D(B)`를 고정한다. 한 이력의 original prefix cells와
다른 이력의 original suffix cells를 합치면 서로 겹치지 않고 전체 target을 채운다.
같은 B에서 suffix는 물리적으로 합법이며, 그 혼합 실행이 q∈Ω와 양립하면 그
multiset과 cover도 완료된 family에 포함된다. 이 대안은 아직 구현된 key가 아니고,
임의 allowlist에는 여전히 적용되지 않는다. 현재 B1 admission에서는 모든 역상
cover의 존재를 이미 입증했으므로 D를 생략할 수 있다.

### B2. Full family가 없을 때의 실제 반례

빈 10×4에 두 후보만 허용한다.

- A: 아래 두 행을 O 다섯 개로, 위 두 행도 O 다섯 개로 채운다.
- B: 아래 두 행을 수평 I 네 개와 O 하나로, 위 두 행도 같은 구성으로 채운다.

B의 한 2행 블록은 x=0..3과 4..7에 바닥 I 두 개를 놓고 그 위에 I 두 개를 쌓은 뒤,
x=8..9에 O를 놓으면 된다. A와 B 모두 앞 다섯 lock으로 아래 두 행을 지울 수 있다.
그 시점의 board는 0, 깊이는 5, 원래 삭제 행도 동일하다. 그러나 A history 뒤의
잔여 종류 언어에는 O 다섯 개만 있고, B history 뒤에는 I 네 개와 O 하나가 필요하다.

따라서 필터된 family의 raw residual을 board/depth로 합치면 원래 허용되지 않은
continuation을 추가한다. 이 예는 일반 finite source에서의 실제 물리 cover 반례이며,
P7P4나 standard 7-bag에서 이 조각 수가 가능하다는 주장은 아니다.

### B3. 공급에 상대적인 완전성은 score memo에 충분하다

현재 producer는 source의 reachable multiset target을 사용하고, 원래 공급의 어느
큐에서도 buildable하지 않은 cover는 observation owner에 전달하지 않는다. 그러므로
B1의 source-independent full physical-family certificate는 그대로 적용되지 않는다.
다음처럼 더 약한 certificate를 따로 만들 수 있다.

1. 원래 유한 Ω, 초기 hold/cursor와 공급 전이가 요청 전체에서 고정된다.
2. Ω의 어느 허용 실행에서든 n lock 동안 나올 수 있는 모든 total multiset을
   packing family가 포함한다. 그 각 multiset에 대한 geometry cover 생산이 완전하다.
3. 각 cover의 dependency 관계가 모든 정확한 물리 행동을 보존한다.
4. cover를 빼는 이유는 Ω 전체에 대한 **정확한 oracle 불가능성**뿐이다. 누적 global
   coverage가 찼다는 이유로 다른 성공 cover나 그 행동을 빼지 않는다.
5. 실제 query의 완료된 producer branch를 모두 합쳤고, 취소·budget·미완료 결과를
   complete 관계로 취급하지 않는다. 추가 tiling/colored/score/history 제한은 없다.
6. B1b의 geometry completeness 범위를 만족하거나, 해당 초기 보드·목표에 대한
   별도 동등한 completeness 정리가 있다.

**정리 B3.** 이 조건에서 실제 상태 `(h,I,cursor,hold)`의 family는 현재 공급에
compatible한 모든 성공 physical continuation을 포함한다. 따라서 그 상태의
최적 정책 값은 과거 h 대신 현재 `(B,k)`와 **기존의 정확한 공급·관측 상태**로 정해진다.

**증명.** 어떤 q∈I에 대해 성공 가능한 physical suffix w를 잡는다. 실제 h와 w를
이어 붙인 실행은 Ω의 q에서 n lock을 성공시키므로 그 total multiset이 packing family에
들어간다. B1b와 geometry 완전성으로 해당 cover가 생산된다. h·w 자체가 q의 성공
witness이므로 정확한 oracle 필터는 그 cover를 버릴 수 없다. 전체 관계가 보존되므로
그 cover의 residual은 h 전체와 w를 모두 허용한다. 반대로 retained 관계의 성공은
실제 물리·공급 성공이므로 거짓 양성이 없다.

관측 정책의 어떤 분기가 I의 어느 큐에서도 성공 completion을 갖지 못하면 그 분기는
값 0이다. 나머지 성공 가능한 공통 행동과 그 모든 성공 suffix는 앞 문단의 논증으로
family에 존재한다. 실패 분기에서 숨은 큐를 추가로 알아낼 필요 없이 같은 정책을
유지할 수 있다. 관측별 분할에 대해 귀납하면 family와 전체 물리 시스템의 최적 값이
같다. 전체 물리 시스템은 현재 B와 잔여 lock 수, 동일한 공급·관측 상태에 대해
Markov이므로 결론이 나온다. □

**이 정리만 직접 사용하는 작은 구현은 정책 값 memo다.** physical node의 score class를 `(board,depth)`로
만들고, `memo_state`가 이미 보존하는 weighted future class, visible window, cursor,
hold, 필요한 terminal history를 그대로 남긴다. 서로 다른 actual observation을
합칠 때는 관측 몫 정리 7의 질량·개수 대응도 그대로 필요하다.

`collect_selected`는 각 원래 residual과 원래 ID domain에서 행동을 다시 평가해야
한다. 한 history의 candidate/prefix 목록을 다른 history의 원본 목록으로 교체하거나,
첫 history의 selected bitset을 복사할 권한은 없다. 특히 raw family는 q와 무관한
suffix까지 서로 같을 필요가 없으므로 B3를 B1의 raw 언어 equality로 해석해서는 안 된다.
실제 물리계 안에서 edge closure를 만들고 원래 Ω와의 root product를 보존하는
별도의 구현은 아래 B5에서 다룬다.

이 절 자체가 board memo를 구현했다는 뜻은 아니다. eligibility
boolean 하나를 full-family 증명으로 대신하지 말고, completed producer의 소유자와
정확한 family·공급 identity가 확인되는 위치에서 별도 권한을 만들어야 한다.

### B4. 현행 finalization에 연결할 구체적인 권한 경계

읽기 검토한 현행 소스에서 `complete_internal(Final)` 진입 자체는 whole-family 증거가
아니다. `complete_external_geometry`와 distributed 경로도 그 함수를 부르고,
`GeometrySearch::finish_external`은 전달받은 candidate count를 완료 수치로 기록한다.
`count_complete`의 초기값 true, `truncated_reason.is_none()`, 혹은 candidate digest도
임의 external 입력이 원래 전체 family였다는 사실을 증명하지 않는다.

공개 query API나 일반 eligibility를 바꾸지 않고 다음 내부 provenance를 추가할 수 있다.

| 소유 위치/사건 | 허용할 상태 또는 권한 |
| --- | --- |
| `WasmExactSearchSession::new_with_external_geometry_catalog` | `external_geometry=false`인 내부 생산만 `InternalPending`; 외부·부분 생산은 `ExternalOrPartial`. |
| `advance`가 내부 enumerator의 실제 `GeometryAdvance::Complete`를 받음 | 원래 provenance가 InternalPending이고 단일 전체 partition이면 `Complete`로 승격. external owner에 `advance`를 호출하여 enumerator 없음으로 받은 Complete는 승격하지 않는다. |
| native `ParallelSearchDecision::Completed` | 처음부터 whole internal family였던 fresh session에서만 허용. `parallel_search::merge_results`의 전체 canonical branch index 검증, count_complete, no truncation 및 모든 observation shard merge 성공 뒤 승격한다. |
| `complete_external_geometry`, `complete_distributed_geometry`, `complete_distributed_worker` | 이번 작은 구현에서는 권한을 주지 않는다. 외부 complete summary의 숫자로 내부 권한을 만들어서는 안 된다. |
| `complete_internal`의 최종 gate | Final role, Complete provenance, count_complete, no truncation, partition count 1, noncancelled를 함께 확인한다. 이후 명시적인 작은 token을 observation 평가에 전달한다. |

이름은 예시이며 필요한 것은 소유 사실이다. `ObservationFamilyProduction` 같은
private enum과 `CompleteObservationFamily` 같은 내부 token이면 충분하다. 완료 시점을
추측하는 새 공개 flag를 받거나 caller가 임의로 certificate를 구성하게 하지 않는다.

첫 지원 범위는 기존 `uses_pc_placement_dependency`에 더해 다음처럼 좁게 둘 수 있다.

- 실제 catalog의 초기 점유 row들이 바닥부터 연속하고 required cells가 그 target
  rectangle 안의 초기 빈칸 전체다. 초기 셀과 required cells는 겹치지 않으며 합집합이
  rectangle 전체, `|B₀|+4n=W·H_b`임을 확인한다. row-mask가 낮은 bit부터 연속한
  1인지로 기저를 검사할 수 있다. 중간 empty row가 있는 초기 필드는 기존 memo로 남긴다.
- `VisibleSeven`, 고정 n, CountUnique, score/execution/replay·colored identity 제한과
  setup/build query가 없다. `allowed_colored_solution_identities().is_none()`은 실제
  `allows_solution_identity` 필터가 없음을 확인한다.
- 처음에는 complete universe와 `StandardBagCoverage::supports`의 fresh-source 조건을
  요구할 수 있다. 이 조건은 초기 hold가 알려진 occupied 상태인 경우도 허용한다.
  hold를 꺼야 한다는 뜻은 아니며, 실제 초기 상태와 projection 계약을 그대로 쓴다.
- 실제 수치 실행은 `ObservationTrie`가 이미 확인한 `uniform_weight.is_some()`의
  count score에 한정한다. 임의 float 가중치의 수학적 동치와 과거 반복합의 bitwise
  동치는 다른 문제다. 토큰이 있어도 이 count certificate가 없으면 기존 key를 쓴다.

`tiling_supply_projection_complete`는 공급 multiset 투영에 관한 기존 수치이므로
geometry 완료 provenance를 대신하지 않는다. 실제 family constructor가 사용하는
`ExactMaterialized`/`ExactSymbolicStandardBag`와 동일 초기 hold/projection의 전제를
활용하되, 부분 생산 여부는 별도 상태로 보존한다.

token 전달 경로는 내부 `complete_internal` →
`BuildUpWorkspace::evaluate_observation_language` → factored view면 충분하다.
Physical node의 첫 residual에서 `symbolic_deleted_rows(prefix)`와
`symbolic_physical_board(prefix, deleted)`를 사용해 B를 복원할 수 있다. 같은 node의
모든 원소는 이미 공통 physical history라는 불변식이 있어, 새 이동 탐색이나 모든
원소의 보드 재검산을 요구하지 않는다. `(B, remaining depth)`의 class는 캐시한다.

이 class는 **score memo에만** 사용한다. raw edge/child ID와 순서를 유지하고,
Frontier/Revealed/tagged kind node는 기존 class를 쓴다. class namespace는 raw node와
tagged kind의 u32 영역과 충돌하지 않게 검증하고 retained bytes를 계측한다.
`memo_reusable`을 새로 true로 바꾸지 않는다. `collect_selected`가 원래 node에서
`best_transition`을 다시 실행하면 실제 최적 정책의 원래 ID를 복원할 수 있다.

다만 **이 재실행만으로 과거 버전과 같은 동률 ID 집합을 보장하지 않는다.** 현행
tie는 `PolicyTransition.child`의 raw u32 ID를 비교하며, factored node는 lazy하게
할당된다. memo hit가 일부 expansion을 생략하면 나중의 ID 할당 순서가 달라질 수
있다. 정확한 optimum count/weight와 선택 정책의 유효성은 보존되지만, literal tie
subset/replay identity까지 요구한다면 stable physical-action tie key 또는 별도의
원래 ID 배정 불변식이 필요하다. stable key를 도입할 때도 그것이 기존 숫자-ID tie와
동일하다는 검증 없이 과거 대표 출력까지 같다고 주장하지 않는다.

### B5. 완전한 producer를 먼저 합친 board DAG의 root product 동치

후보별 residual을 그대로 보관하는 대신, 각 typed active prefix를 실제
`(현재 B, remaining)`으로 보내고 물리 edge를 그 node에서 union한 graph G를 생각하자.
이것은 정책이 방문한 branch의 prefix만 그때그때 합치는 online memo가 아니다.
producer가 필요한 hidden 깊이의 **모든** active prefix와 물리 edge를 등록하고,
모든 worker의 관계를 합친 뒤 graph를 freeze하여 정책 평가를 시작한다.

F를 등록된 typed candidate들의 원래 물리 언어 합집합, M을 요청이 허용하는 실제
PC 물리계라 하자. M의 상태는 B와 남은 lock 수이며, initial board·entry 규칙·실제
ordered-kick authority는 같은 요청 owner에 고정한다. 이번 범위에는 tiling allowlist,
색상, 점수, finesse·입력 이력, 특정 원래 solution identity 제약이 없다.

prefix의 projection은 기존 `symbolic_deleted_rows`, `symbolic_physical_board`와
`symbolic_frame`이 제공한다. typed edge는 현재 B에서 실제 도달·lock이 가능함을
보증하며 다음 상태는 반드시

\[
  (B,r)\xrightarrow{(p,C)}
  (\operatorname{clear}(B\cup C),r-1)
\tag{T}
\]

이다. C는 그 p의 현재 네 셀이며 B와 겹치지 않는다. accepting은 `B=0 && r=0`이다.
동일 `(B,p,C)`에서 다른 child board를 얻으면 nondeterministic union으로 덮지 않고
projection 계약 오류로 거부한다.

**보조정리 B5a.** `L(F) ⊆ L(G) ⊆ L(M)`.

F의 모든 prefix/edge가 G에 들어가므로 첫 포함관계가 성립한다. G의 경로가 서로
다른 candidate의 edge를 번갈아 사용하더라도, 식 T의 각 행동은 그 시점의 실제 B에서
합법이다. 고정된 물리 규칙은 과거 candidate 이름을 읽지 않는다. 따라서 root부터
귀납하면 그 경로 전체가 실제 물리 실행이고, accepting 조건 때문에 PC로 끝난다.
이것이 두 번째 포함관계다. □

AΩ를 원래 queue ID, 초기 hold/cursor, 실제 supply action과 projection/EOF 규칙에
compatible한 실행쌍 집합이라 하자. B3의 완료된 원래-source family certificate는
`L(M) ∩ AΩ ⊆ L(F)`를 준다. 실제 q∈Ω의 성공 실행을 B1b로 lift한 cover는 생산되고,
그 실행 자체가 oracle 성공 witness이므로 필터로 제거되지 않기 때문이다. 따라서

\[
\boxed{L(F)\cap A_\Omega=L(G)\cap A_\Omega=L(M)\cap A_\Omega}.
\tag{P}
\]

**정리 B5.** 식 T의 soundness와 완료된 B3 certificate를 만족하면, freeze된 G를
원래 Ω의 observation/supply automaton과 product한 root 최적 정책 값은 F의 값과 같다.

G의 정책도 공개 전에 실제 `(kind,cells)`와 hold action을 먼저 선택한다. 그 뒤
원래 observation automaton이 같은 큐 정보를 공개한다. G의 정책이 q에서 성공하면
식 P가 같은 성공 실행의 F witness를 준다. 반대로 F의 모든 성공 실행은 G에 있다.
동일한 observable history에서 같은 행동을 선택하며, 성공 completion이 없는
분기는 값 0으로 남긴다. 깊이별 관측 합과 행동 최댓값으로 root 동치를 얻는다.
source와 무관한 raw 언어가 커질 수 있다는 사실은 이 product 동치에 모순되지 않는다. □

현행 코드에서 이 포함관계를 적용하는 연결은 다음과 같다. 이 표의 완전성은
제품에 이미 존재하는 exact-cover/typed-dependency의 정확성 계약을 전제로 하며,
추가 board quotient가 그 계약 밖의 후보나 필터를 끌어오지 않는지 확인한 것이다.

| 확인할 계약 | 실제 연결과 보존 이유 |
| --- | --- |
| 초기 보드·목표와 전체 원래 공급 | [`buildup.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs)의 `board_observation_plan`이 초기 occupied-row prefix, full-row 없음, `required=field\\initial`, 고정 n, 실제 complete fresh Standard7Bag storage/weights/cardinality를 확인한다. 이름만 같은 generic·filtered·observed 공급은 이 인증을 얻지 못한다. |
| 모든 q-compatible total multiset | [`result.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/result.rs)의 session 생성자가 catalog의 면적에서 얻은 n, 같은 initial hold 및 `packing_hold_projection`을 `packing_multiset_family_for_execution`에 전달한다. [`bag_multiset_reachability.rs`](../../crates/clearra-supply/src/pattern_universe/bag_multiset_reachability.rs)는 shared supply automaton의 정확히 n lock 전이를 사용한다. 무한 fresh-bag projection에 유한 source N보다 많은 multiset이 생길 수는 있어도 실제 q의 n-lock multiset은 빠지지 않는다. surplus의 실제 성공 여부는 원래 Ω와의 coverage product가 판정한다. |
| complete cover 생산 뒤의 후보 필터 | [`buildup/placement_dependency.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup/placement_dependency.rs)는 정확한 종류 언어와 원래 공급의 product가 buildable인 후보의 **전체** physical-complete active relation을 등록한다. `CandidateWitnessMode::for_candidate`는 observation 요청에서 즉시 Disabled이므로, 다른 cover가 원래 ID를 이미 덮었다는 이유로 이 cover의 행동을 생략하지 않는다. typed proof의 거부·자원 오류를 불가능 후보로 조용히 바꾸지 않는다. |
| 실제 query가 추가 필터를 요구하지 않음 | admission은 colored/tiling allowlist, score·execution constraints, finesse/replay evidence, Setup/Build, solution별 출력과 CountAll을 제외한다. 공개 work/memory cap은 dependency eligibility에서 제외되며, 내부 자원 실패·취소·미완료 생산은 `Complete` 권한을 얻지 못한다. |

n은 **lock 수**, N은 **원래 source draw 수**이며 둘을 같다고 가정하지 않는다.
초기 hold 점유, 빈 hold의 첫 Store가 쓰는 추가 draw, materialized EOF에서의
held terminal release, 확정된 bag 마지막 조각의 projection은 기존 공급 automaton과
observation 상태에 남는다. `SearchProblem::resolve_supply_window`와
`initial_hold_automaton_for`가 만든 같은 계약을 packing·coverage·policy가 공유한다.
전형적인 fresh Standard7Bag query는 cursor 0과 fresh bag remainder에서 시작한다.
후속 코드가 그 초기 상태를 임의로 바꿀 수 있게 되면 실제 source storage 인증만으로
대체하지 말고 공급 전이와 source ordinal의 정합성도 다시 인증해야 한다.

이 논증은 **G의 edge가 실제 물리계 안에 있다는 별도 soundness 증명**을 사용한다.
임의 필터 family의 residual을 보드 key로 바꾼다는 이유만으로는 성립하지 않는다.
B2의 allowlist 자체가 query 제약이면 stitching한 경로가 M에 들어가지 않을 수 있어
첫 단계부터 실패한다. source 때문에 cover를 줄인 경우에는, Ω에서 성공하는
stitching의 cover도 complete 생산에 포함되어야 하므로 식 P가 적용된다.

#### 조기 reveal과 종류 suffix의 union

K(B,r)를 그 보드·깊이에 등록된 각 prefix의 정확한 종류 suffix 언어 합집합이라 하자.
각 word는 실제 B에서 r lock 후 PC로 끝나는 물리 witness를 갖는다. 원래-source reveal
인증 뒤에 그 word를 선택하면 현재 history h에 붙인 실행도 합법이다. 역방향으로
새 h 뒤의 q-compatible suffix w가 성공한다면 h·w 자체가 실제 query 성공이므로,
complete certificate가 **그 전체 실행의 일관된 cover**를 F에 포함시킨다. 해당
prefix의 suffix root가 K(B,r)에 있으므로 필요한 성공을 잃지 않는다.

이것은 reveal 전에 kind-only 선택을 허용하지 않는다. 기존 `fully_revealed_node`
인증 순서를 유지하고 모든 suffix root의 remaining depth와 pinned arena를 확인한다.
A4 하계는 모든 실제 hold/cursor 경로에서 증명되므로 이 graph에서도 그대로 사용할 수
있다. H=n의 경계는 실제 accepting terminal이며, H=0도 원래 인증 뒤에만 K를 사용한다.
현행 board admission은 H=0이면 기존 경로를 사용한다. 그 사실은 H=0에 대한 정리를
부정하는 것이 아니라 새 owner를 구성할 필요가 없는 범위를 제외한 것이다.

[`queue_observation_policy.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy.rs)의
`best_revealed_alternative`는 positive uniform 원래 질량, fixed horizon 및 별도
terminal supply target 없음에서만 사용된다. reveal 인증 뒤에는 성공한 종류 root
하나가 해당 information set의 **모든** ID에 대해 성공하거나 전혀 성공하지 않아야
한다. 부분 개수가 나오면 다음 root와 부분집합을 임의로 합치지 않고 계약 오류로
거절한다. 따라서 종류 root의 순서가 달라도 성공한 information set 전체의 질량은
같다. 이 fast path를 임의 weighted 또는 terminal-history 제약에 확대하는 권한은 없다.

#### 구현·실패·출력 계약

현재 구현한 연결은 parent session에서 `BoardObservationPlan` Arc 하나를 만들어
root workspace와 모든 native worker에 공유하는 방식이다. node key에서 생략한
initial/catalog/rule/source 계약은 같은 요청 owner와 plan identity로 고정한다.
serial 첫 whole-family advance와 fresh native handoff만 이 plan을 설치한다.
외부 후보, distributed partial, partitioned 작업은 기존 관계 표현을 유지한다.

`ObservationFamilyProduction::{ExternalOrPartial,InternalPending,Complete}`는
실제 내부 `GeometryAdvance::Complete` 또는 전체 canonical branch와 모든 shard merge가
검증된 native 완료에서만 Complete가 된다. final role, count complete, truncation 없음,
단일 partition, 취소 아님도 최종 평가 시 다시 확인한다. eligibility helper나 외부
완료 숫자로 이 권한을 만들지 않는다.

실패 후 session을 다시 호출하더라도 authority가 복구되면 안 된다. native handoff에서
geometry를 옮긴 뒤 실패하거나, serial geometry가 내준 candidate의 검증이 실패하면
provenance를 내려야 한다. placeholder geometry의 Complete나 다음 candidate 뒤의
traversal 종료가 빠진 관계를 완전하다고 인증해서는 안 된다. 이미 old proof를 폐기한
board owner가 중간에 중단되면 exact 완료 확률을 내지 않는다.
현행 `complete_internal`은 이런 owner를 정책 평가에 넘기지 않고
`observation_family_incomplete`로 표시한다. `coverage_calculated`,
`probability_calculated`, `probability_complete`를 false로 두며 `coverage_probability`는
`not-calculated`로 출력한다. 중간 저장소의 빈 bitset을
완료된 성공확률 0%로 오인하게 해서는 안 된다. 이 출력 경계는
`observation_board_incomplete_family_never_reports_an_exact_zero_probability` 회귀로
검증했다.

이 표현은 CountUnique의 Boolean root 정책 값용이다. stitched 경로가 특정 저장
candidate의 대표 경로인 것은 아니므로 tiling/path/execution evidence를 이 권한으로
대신하지 않는다. 이번 physical action tie는 `(supply action,kind,cells)`의 의미상
순서를 사용하며, 기존 numeric-child ID tie를 **의도적으로 변경**한다. 같은 optimum의
정책 중 다른 정책과 다른 성공 original-ID subset을 선택할 수 있다. 원래 선택 bitset까지
동일하다는 주장은 하지 않는다. 실제 admission은 동일 positive uniform weight를
확인하므로 exact covered count가 같으면 optimum weighted mass도 같다. 임의 가중치에서
count equality만으로 mass equality를 결론내리지는 않는다. 기존 13개 factored metrics의
의미는 유지하고 board graph의
candidate, node, edge, suffix root, prefix visit, edge attempt, work를 별도로 기록한다.

등록 비용은 자동으로 줄어드는 것이 아니다. 기존 lazy source scan을 모든 필요한
active prefix/edge의 한 번의 eager scan과 hash/merge로 바꾼다. unique board/edge가
많으면 여전히 큰 비용이다. 생산·freeze·정책 평가 시간, candidate/proof 장기 보유
절약, worker 및 coordinator peak를 각각 비교해야 한다. 이 절은 all-path 동치
증명이며, 아직 완주하지 않은 fixture의 성능 성공을 뜻하지 않는다.

### B6. q4: 공개 뒤 종류 언어의 lazy deterministic union

Board의 같은 `(B,r)`에 모인 canonical kind roots를 R이라 하자. q3는 reveal 인증 뒤
root마다 membership을 반복했다. q4는 **이미 인증된** 그 집합을 하나의 lazy union
state로 공유한다. 서로 다른 숨은 physical footprint를 합치는 변경은 아니다.

**정리 B6.** 같은 remaining depth의 정확한 종류 roots에 대해 다음 derivative가
원래 union의 word 언어를 보존한다.

\[
L(U(R))=\bigcup_{x\in R}L(x),\qquad
\delta_p U(R)=U\bigl(\{y:\exists x\in R,\ x\xrightarrow p y\}\bigr).
\]

빈 word 수용은 member의 accepting OR다. 비어 있지 않은 word `p·w`는 어떤 member의
p-edge 뒤에 w가 있는 경우와 정확히 같다. 남은 길이에 대한 귀납으로 equality가
성립한다. Canonical node가 같은 kind의 child를 여러 개 가지더라도 **모두** 모으면
이 증명은 그대로다. root 하나를 미리 택하거나 같은 kind의 첫 child만 취하면
성립하지 않는다. □

[`queue_observation_kind_union.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_kind_union.rs)의
`append_kind_children_bounded`는 모든 해당 edge를 모은다. 기존 eager union도 같은
child 수집 단위를 사용한다. `KindUnionArena::intern_scratch`는 sorted unique roots와
remaining depth를 hash하고 전체 집합 equality로 충돌을 확인한다. 모든 member가
같은 r이고 accepting이 정확히 `r=0`인지 검증한다. Singleton은 기존 kind node를
재사용하고, empty derivative에는 edge가 없다. `prepare_inner`는 요청 kind만 전개하며
이미 제공한 edge index는 바꾸지 않는다. 같은 board의 reveal alias도 한 번만 만든다.

Board view는 raw board, revealed-board reference, union reference 및 canonical kind
reference의 namespace를 분리한다. KIND bit를 먼저 해석하고 union bit와 raw board
범위가 겹치지 않게 제한한다. Kind arena identity는 view 전체에서 고정되고 union
node는 append-only다. 실제 family와 새 union의 node·edge·member·work 합에 기존
상한을 적용한다. 포함관계 pair 비교나 budget을 이유로 root를 임의 제거하는 과정은 없다.

이 word equality가 policy equality가 되는 조건은 reveal 권한이다. 현재 board 범위는
fixed horizon, positive uniform 원래 weights 및 별도 terminal supply target 없음이다.
원래 evaluator가 모든 관련 draw를 알았다고 인증한 뒤에는 성공 root가 그 information
set의 모든 ID를 수용하거나 전혀 수용하지 않는다. 원래 source·hold·cursor·EOF를
그대로 product하므로 union의 existential word 선택도 같은 all-or-none 결과를 낸다.
Terminal-held release의 0-draw 전이와 accepting-child 검사는 변경하지 않는다.
r=1이면 그 다음 child가 terminal이어야 하고 r=0은 기존 terminal이며, 원래 bag의
확정 마지막 조각은 trie projection이 그대로 담당한다.

같은 frozen physical producer를 사용하는 q4 mode 0/1 비교에서는 공개 전 행동별 정확한 값이
같고 semantic action tie도 같으므로 선택한 physical prefix가 같다. 공개 뒤에는 어느
성공 종류 경로를 택해도 같은 information set 전체 ID를 수용한다. 따라서 **q4 union
on/off의 original-ID bitset도 같다.** 이는 과거 numeric-child tie를 쓰던 제품까지
동일하다는 주장이 아니다. Memo의 numeric key와 entry 수, 공개 뒤 hold witness나
대표 종류 경로는 같을 필요가 없다. Arbitrary weighted/terminal-target 경로로의 지원
확장도 이 board 최적화에서 주장하지 않는다.

### B7. q5: 첫 fresh bag 안의 prefix 종류 상한과 productive filter

이 절은 q5 구현과 전 경로 증명이다. 78개 observation 검증, 실제 8-lock source
prefix mode 0/1의 동일 coverage, 10-lock work 상한 및 11-lock 제한 probe timeout은
§9에 구분해서 기록한다. 실제 complete fresh source 인증은 A4와 같은 owner를
사용한다. E는 초기 empty hold가 허용될 때 1, 그 외에는 0이고 H는 hidden horizon이다.
다음 추가 조건을 만족할 때만 source prefix 제한을 사용한다.

\[
c_0+H+E\le7.\tag{F}
\]

처음부터 held인 종류 p가 있으면 a_p=1, 아니면 a_p=0이라 하자. Prefix S에 놓인
종류 p의 수를 C_p(S)라 정의하면 필요한 조건은

\[
C_p(S)\le1+a_p\quad(\forall p,\ |S|\le H).\tag{K}
\]

**정리 B7a.** 모든 원래 Ω의 실제 실행 prefix는 식 K를 통과한다.

k lock 뒤 consumed draws는 A4의 보존식으로 `cursor≤c₀+k+E`다. 식 F 아래에서
첫 H lock에 사용할 수 있는 원래 source 위치는 모두 첫 fresh bag 안이다. 그 구간에는
각 종류가 최대 하나 있고, 별도로 제공된 초기 held piece만 하나 더 놓을 수 있다.
Hold swap이나 store는 이 조각 수를 늘리지 않는다. 0-draw terminal-held release도
이미 있던 held piece 하나를 놓을 뿐이며, projected final bag piece는 같은 bag의
유일한 마지막 종류다. 따라서 식 K가 모든 source/hold 경로의 필요조건이다. □

[`buildup.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs)의
`board_observation_plan`은 실제 full fresh source를 확인한 뒤 checked 산술로 식 F를
검사한다. 통과하면 `prefix_kind_limits=[1;7]`에 initial-held 종류만 1을 더해 같은
request plan Arc에 넣는다. 통과하지 않으면 None으로 기존 범위를 유지한다. 실제
초기 cursor를 식에 포함하므로 c₀=0을 암묵적으로 가정하지 않는다. Generic·filtered
source의 이름이나 candidate의 성공 집합으로 이 권한을 만들지 않는다.

**단순 cap 검사만으로는 충분한 producer 구현이 아니다.** Cap-valid parent도 모든
child가 제외되면 새 dead node가 된다. 기존 `freeze`는 nonterminal hidden node에
outgoing edge가 있다고 확인하므로 그런 node를 그대로 등록하면 정당한 pruning을
family 오류로 오인한다. Q5 `register_inner`는 다음 reverse productive pass를 쓴다.

1. 각 종류의 concrete operation bitmask를 한 번 만든다. 전체 multiplicity가 cap
   이하인 종류는 prefix 검사에서도 제외한다.
2. Active prefix를 **숫자 mask 역순**으로 처리한다. 깊이가 H인 cap-valid prefix를
   경계로 보관한다. 깊이 H 미만에서는 cap을 통과하고 보관된 live child가 하나 이상
   있는 prefix만 남긴다.
3. 남긴 prefix의 실제 frame/board와 정확한 child edges를 같은 pass에서 등록한다.
   Suffix root는 여전히 `depth≥L`일 때만 보관하며 원래 정확한 종류 언어를 사용한다.

**정리 B7b.** 이 pass는 root에서 유효 H frontier까지 이어지는 모든 필요한 경로를
보존하고, 새 orphan/dead node를 만들지 않는다.

Edge `S→S∪{i}`의 child는 항상 숫자가 크므로 역순에서 먼저 처리된다. 귀납적으로
보관된 prefix는 cap-valid H frontier에 도달하는 것과 동치다. Typed active relation은
root-reachable이며 terminal-productive다. 종류 개수 상한은 downward-closed이므로
유효 H-prefix의 원래 root 경로에 있는 모든 ancestor도 cap-valid다. 그 경로가 역순
귀납으로 모두 보관되므로 고립된 H 노드만 추가한 채 root가 없는 경우도 생기지
않는다. 유효 H frontier가 전혀 없는 후보는 아무 node나 suffix를 추가하지 않는다. □

이 정리를 B5에 연결하면 실제 q 성공 실행의 lift cover에서 모든 k≤H prefix와 edge,
H suffix가 남는다. 따라서 `L(M)∩AΩ⊆L(F_pruned)`가 유지된다. Board stitching이
어떤 다른 이력에서 식 K를 위반하는 종류 조합을 만들더라도 원래 supply automaton이
그 조합을 실행할 수 없어 product에서 거절한다. 식 K는 물리적인 불가능성의 증거가
아니며, **원래 Ω에 대한 prefix 불가능성**이다.
Q5 이후에는 raw `L(F)⊆L(G)`를 무조건 그대로 쓰지 않고 B5a를 먼저 F_pruned에
적용한다. 원래 공급 안에서만
`L(F)∩AΩ = L(F_pruned)∩AΩ = L(G_pruned)∩AΩ = L(M)∩AΩ`가 성립한다.

후보 oracle buildability, typed physical relation 자체와 full suffix 종류 언어는
바꾸지 않는다. 특히 H 뒤에는 두 번째 bag의 반복 종류가 합법일 수 있으므로 원래
전체 proof에 같은 cap을 적용해서는 안 된다. `source_pruned_prefixes`와 후속 경계에
도달하지 못한 `dead_source_prefixes`도 별도로 기록한다. 기존 prefix 방문 자체와
필요한 child 존재 검사는 남으며, 저장하지 않는 frame/node/suffix/edge 비용을 줄인다.

조합론적 예로 multiplicities `(2,2,2,1,1,1,1)`인 n=10의 전체 operation subsets에서는
깊이 3의 24/120=20%, 깊이 4의 81/210≈38.57%가 cap을 위반한다. n=11의
`(2,2,2,2,1,1,1)`에서는 깊이 5의 294/462≈63.64%다. 이 수치는 **전체 subset**의
비율이다. 실제 active relation, 다른 held multiset, board coalescing 및 dead-prefix
제거율과 같다고 가정하거나 실제 suffix-root 절약률로 보고하지 않는다.

### B8. q6: H 경계에만 suffix를 보유하고 조기 공개 뒤 물리 간선을 재사용

Q5는 최소 공개 깊이 L부터 H까지 모든 보드에 kind suffix root 집합을 보유한다.
Q6는 그중 H 경계의 집합만 보유한다. 원래 evaluator가 H 이전에 reveal을 인증해도
같은 raw board node를 반환하고, 이미 보유한 물리 행동을 계속 따라간다. H에서만
기존 exact kind union으로 전환한다. 이는 원래 reveal 조건을 늦추거나 추가 정보를
숨기는 정책 변경이 아니라, 알려진 공급의 성공 witness를 얻는 표현 변경이다.

**정리 B8.** B5의 완료 권한과, cap을 사용하는 경우 B7의 공급 조건 아래에서 q5와 q6는 root 정책 값,
원본 ID별 selected coverage, 고정 완성 깊이를 보존한다.

원래 q와 실제 이력 h에서 성공 completion이 존재하면 그 전체 실행의 lift cover가
완료 family에 포함된다. 그 실행의 모든 k≤H prefix는 cap이 있으면 B7의 조건을
통과하고, cap이 없으면 원래 active 경로로서 reverse productive pass에 남는다.
따라서 q6는 h의 현재 B에서 H까지의 물리 경로를 따라
그 성공 suffix에 도달한다. 반대로 q6의 성공 경로는 합법인 물리 edge와 같은 B의
실제 PC suffix witness를 이어 붙인 실행이다. 원래 supply automaton이 이를
받아들이면 원래 q에서 실행 가능한 PC이므로 q5에도 성공 completion이 있다.
동치는 임의의 filtered family에 대한 raw suffix equality가 아닌
`L(G_q5)∩AΩ = L(G_q6)∩AΩ`이다.

조기 reveal의 권한은 이후에도 유지된다. 원래 미래가 결정됐다면 그 descendant의
미래도 결정된다. 관련 draw 상한을 `R=c+r+e`로 쓰면 Current와 SwapHeld 뒤 R은
그대로다. StoreCurrent 뒤에는 `c'=c+2, r'=r−1, e'=0`이고 이전 e=1이므로 역시
R이 같다. 관측 깊이는 줄지 않는다. 이미 알려진 관련 공급 아래에서는 성공
witness 하나가 정보집합 전체에서 유효하므로 H까지 추가 물리 선택을 해도
성공 ID의 일부만 고르는 선택 문제가 생기지 않는다.

최초 reveal 이전에는 같은 물리 행동, 같은 행동별 값, 같은 semantic tie
`(supply action, kind, cells)`를 사용한다. 최초 reveal 이후의 값은 정보집합 전체
성공 또는 전체 실패다. 따라서 실제 witness나 공개 뒤 행동 순서가 달라질 수는
있어도 원래 ID bitset은 같다. Memo에도 같은 raw board identity, source future
class, visible window, cursor, hold가 남으므로 kind alias와 물리 상태를 섞지 않는다. □

구현에서는 등록과 freeze가 같은 suffix 필요 깊이를 사용해야 한다. H 미만 반환은
`fully_revealed_node`의 suffix-empty 검사보다 앞에 둔다. H<n에서는 기존 reveal
인증 이후에만 kind union으로 전환하며, H=n에서는 accepting 선처리를 유지한다.
Terminal held release는 accepting child로만 가므로 H<n인 H 이전에 나타나지
않는다. 내부 H=0 fixture에서도 root kind 전환 의미를 유지한다. 상한·워커·원래
Ω·candidate oracle 관계는 그대로이며, 실제 비용 효과는 mode 0/1로 별도 계측한다.

### B9. q7 실험: 보유한 board 집합에서 legal physical edge 재구성

Q6와 같은 source-cap productive board nodes와 H suffix roots를 생산하되,
candidate마다 모든 physical edge를 반복 삽입하지 않는 방안이다. 생산자는 각
prefix의 retained child 존재 판정까지 유지한다. 전체 생산 완료 뒤 finalizer가
`depth<H`인 각 `(B,r)`에서 legal locks를 구하고, 정확한 child
`(clear(B∪C),r−1)`가 이미 보유한 board index에 있을 때만 edge를 만든다.
이 절은 **기본 mode 0으로 구현·검증한 q7 실험의 증명과 계약**이다. 전체 및 focused
검증의 정확한 소스 범위는 §9에 둔다. 제품 기본 채택, 측정 이득 또는 10/11-lock
완주를 아직 주장하지 않는다.

**정리 B9.** 같은 물리 authority와 B5 완료 권한 아래에서 q7의 graph G₇은 q6의
graph G₆과 원래 Ω의 root 정책 값 및 selected original-ID 집합을 보존한다.

G₆의 모든 edge는 같은 현재 B에서 legal인 grounded `(kind,C)`이고 두 endpoint가
보유된 node다. 따라서 완전한 finalizer가 그 edge를 포함한다. 반대로 G₇의 모든
edge는 실제 lock과 deterministic clear이므로, 추가 edge들을 조합해도 실제 물리
실행이다. H suffix의 실제 PC witness와 연결한 경로가 원래 q에서 성공하면,
전체 실행의 lift cover가 완료 family에 있다. 그 cover의 prefix는 cap이 있으면
B7을 통과하고, cap이 없으면 원래 active 경로로 남으므로 같은 성공 실행이 G₆에도
포함된다. 따라서

\[
L(G_6)\cap A_\Omega=L(G_7)\cap A_\Omega.
\]

더 나아가 G₇에만 있는 physical 행동이 원래 Ω의 reachable product state에서
양의 값을 가진다고 가정하면, 그 행동을 포함하는 q의 성공 실행이 존재한다.
그 실행의 lift cover는 G₆에 같은 `(B,kind,C)` edge를 이미 넣었어야 하므로 모순이다.
따라서 새 행동은 해당 원래 product에서 성공 값을 더하지 않는다. 기존 positive
행동의 값과 semantic tie `(supply action,kind,cells)`는 같으며, 값이 전부 0인
상태에서는 선택 ID를 추가하지 않는다. H 뒤에는 같은 suffix와 B6/B8의 all-or-none
정보집합 계약을 쓰므로 original-ID bitset도 같다. □

초기 occupied-row prefix, `h+d≤H_b` 및 split inverse-clear catalog의 B1 조건은
그대로다. 새 성공 경로 자체에 그 lift를 적용하므로 deleted-row identity D를
새 board key로 추가할 필요는 없다. 임의 tiling allowlist·history·score·finesse
제약이나 불완전 producer에서는 이 정리를 사용하지 않는다.

#### Finalizer의 완전한 대상과 partial reachability 계약

전체 lock 탐색을 항상 `desired=None`으로 실행할 필요는 없다. 다음 절차도 같은
edge 집합을 정확하게 만든다.

1. 표준 조각의 **모든** full-field 4-cell grounded pose를 열거한다. 현재 B와
   겹치지 않고 정확한 clear child가 retained index에 있는 pose만 남긴다. 같은
   `(kind,cells)`를 만드는 회전 alias들은 하나의 행동에 속한다.
2. 기존 hard-drop corridor나 같은 입력·authority의 exact cache가 주는 양성
   witness는 그대로 재사용한다. 입증되지 않은 나머지 anchor를 desired 집합 D로
   만든다. Alias 하나가 legal이면 해당 physical 행동이 legal이다.
3. 같은 request workspace의 template/scratch에서
   `search_reachable_locks(..., Some(D))`, 즉 `DesiredLocks::All(D)`를 실행한다.
   `exhaustive=false`는 **요청한 D 전체를 찾았을 때만** 완전한 양성 판정으로
   사용할 수 있다. `exhaustive=true`이면 남은 미발견 anchor에 대한 음성도 확정된다.
   단 하나를 찾는 `Any` 종료나 취소·resource refusal을 나머지의 음성으로 쓰지 않는다.

`None`의 exhaustive 탐색은 이 절차의 충분한 특별한 경우다. 일부 desired만
물어본 결과를 전체 pose 집합의 부재 증명으로 확대하지 않는다. 캐시의 음성을
사용한다면 같은 complete 입력 범위의 음성 certificate여야 한다.

[`reachability.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability.rs)의
`ReachabilityWorkspace::template`가 선택하는 같은 원래 catalog width/height,
all-sky seeds, normalized ordered table와 ceiling을 사용한다. Imported profile은
같은 `CompactKickAuthority`에 bind된 template를 재사용한다. Profile ID만으로
`ReachabilityTemplate::compile`을 다시 부르면 builtin table이 선택되므로 imported
authority의 대체가 될 수 없다. Occupied height나 남은 row 수로 별도 높이를 만들지도
않는다. 실제 이동·충돌·first-success와 cell/child 검증은 기존 kernel을 공유한다.

생성 edge는 `(kind,cells)`로 dedup·정렬해 semantic tie를 유지한다. 각 retained
nonterminal node의 기존 성공 child가 사라지면 완료 family를 거절해야 하며,
finalizer 실패를 성공확률 0으로 바꾸지 않는다. BFS visited-state와 대상 pose 수,
양성 재사용·남은 query·생성 edge, freeze 시간·peak를 별도로 계측한다. Candidate별
반복을 unique board별 탐색으로 바꾼 효과는 실제 작업량으로 확인하며, BFS 전체를
상수 1 work로 세거나 기존 resource cap을 높여 성능 이득으로 보고하지 않는다.

#### B9 따름정리: q9의 층별 혼합 재구성

q7 실제 P7P4는 전체 후보를 생산했지만 global edge finalization 중 work 상한에
도달했다. 따라서 q9 실험은 EDGE=0의 원래 후보별 edge 등록과 EDGE=1의 모든 hidden
층 재구성 사이에 **EDGE=2**를 둔다. `depth+1<H`인 앞쪽 층만 global legal-edge로
재구성하고, `depth=H−1`에서는 원래 candidate의 typed proof edge를 등록·merge한다.
보유 board nodes와 H suffix roots, source cap, 완료 권한 및 모든 자원 상한은 같다.
q9 소스와 회귀는 `dcf06bc`에서 기본 84개·parallel/local A/B 86개 관측 검증에
포함되었다. 실제 8-lock EDGE=0/1/2 비교도 완료했지만, 10-lock은 freeze 뒤 정책
work 상한으로 미완료다. 구현·정확성 근거와 큰 입력의 완주는 구분한다.

**따름정리 B9h.** 같은 Plan에서 재구성할 층 집합을 고정하고 producer와 finalizer가
그 판정을 공유하면 q7의 root 정책 값·원래 ID 보존은 임의의 층별 혼합에도 성립한다.
각 재구성 층은 기존 모든 edge를 포함하고 실제 물리 edge만 추가한다. 나머지 층은
기존 typed edge 자체를 유지한다. 따라서 혼합 graph G₂도 `L(G₀)⊆L(G₂)⊆L(M)`이며,
B5/B7의 원래 Ω 상대 완전성으로 product 언어가 같다. 추가 행동이 원래 reachable
product에서 양수라면 그 성공 cover가 해당 층의 원래 edge도 생산했어야 한다는
B9의 모순 논증을 그대로 적용한다. 같은 positive 행동과 semantic tie가 유지된다. □

H=0과 H=1에서는 mode 2의 재구성 층이 비어 있다. 이 경계에서 unsigned 뺄셈으로
층을 판정하지 않으며, 등록·재구성의 공통 predicate와 같은 immutable Plan Arc를
사용하고 freeze에서는 완료 latch를 확인한다. 실제 코드는 `BoardObservationPlan::should_rebuild`를 producer와
finalizer가 함께 쓰고, pose 준비는 첫 재구성 부모에서만 수행한다. 따라서 H=0/1은
pose 배열·BFS 작업을 만들지 않는다. `requires_edge_closure`는 Off 이외의 mode를
포함하므로 mode 2도 같은 요청의 킥·conditioned 설정을 받는다.
mode 2의 이득은 정리가 보장하지 않는다. 마지막 hidden 층은
보유 board 수가 많아 unique-board 도달성 비용이 클 수 있고, 기존 typed edge는 이미
이동 증명을 갖고 있다. 반대로 그 층의 candidate별 중복 삽입 비용은 다시 지불한다.
이 두 실제 비용을 구분해 같은 cap에서 비교하며 기본 EDGE=0은 유지한다.

### B10. q8: 전체 count가 증명된 정보집합의 ID 직접 수집

이 절의 변경은 **q9의 `dcf06bc`에 반영되어 focused 검증과 실제 8-lock
COLLECT_FULL=0/1 비교를 완료한** 상태다. 검증 범위와 미완료 큰 입력은 §9를 따른다.
[`queue_observation_policy.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy.rs)의
`collect_selected`에서 기존 accepting 처리와 `value.pattern_count == 0` 반환 뒤,
`best_revealed_alternative`를 다시 실행하기 전에 다음 조건을 확인한다.
`language.fixed_horizon() == Some(n)`이고 memo 값의 양수 `pattern_count`가
**현재 원래** `state.observation_node`의 `subtree_count`와 같으면
`trie.collect_patterns`로 그 subtree의 원래 ID를 모두 모으고 선택 깊이를 n으로 갱신한다.

**정리 B10.** 기존 solve와 memo의 정확성 계약 아래에서 이 변경은 원래 선택 ID,
성공 개수·가중 질량 및 min/max accepted depth를 보존한다.

현재 정보집합 I를 observation subtree의 terminal entry 집합이라 하자. accepting의
`acceptance_value`는 terminal supply target까지 적용한 I의 부분집합을 센다.
각 transition은 `collect_revealed_descendants`가 만든 서로 겹치지 않는 subtree의
선택 집합들을 합한다. `best_transition`은 score와 기존 tie에 따라 이 합집합 중
한 action의 결과를 고른다. 이미 허가된 revealed-alternative 경로도 기존 검사를
통과한 한 결과를 선택한다. 따라서 깊이 귀납으로 solve가 반환한 count c는 어떤
실제 선택 집합 S⊆I의 크기이며, 항상 `c≤|I|`다. 양수 c가 `subtree_count=|I|`와
같으면 유한 집합의 cardinality에 의해 S=I다. 이 경우 모든 최적 동률 선택이 같은
전체 ID 집합을 내므로 구체적인 행동 선택을 다시 구성할 필요가 없다. □

이는 weight가 같은지 또는 미래 공급이 공개됐는지로 전체 성공을 추정하는 shortcut이
아니다. **이미 기존 solve가 계산한 count의 등식**만 사용하므로 비균등·0 가중치와
terminal target도 기존 판정을 유지한다. 가중 score의 반복합과 `total_count`는
바꾸지 않으며, terminal target을 새로 생략해 solve하거나 현재 상태에서 다시 판정하지
않는다. EOF와 held release 역시 기존 공급 automaton 및 acceptance에 남는다.

Memo class에서 원래 ID를 추측하지 않는다. `memo_state`의 future signature는
`subtree_count`를 포함하고, 기존 visible window·cursor·hold·terminal bag 이력을
유지한다. 같은 class의 값이라도 수집은 현재 **원래 trie node**에서 수행한다.
`PatternPiecePositionIndexCompileSession::new_for_pattern_ids`는 엄격 증가하는
global ID를 요구하므로 local/global 중복 mapping은 허용하지 않는다. 동일 sequence를
가진 서로 다른 원래 ID들은 별개 terminal entry로 세고, `collect_patterns`도 모두
복원한다. 최종 bitset의 원래 ordinal 의미는 바뀌지 않는다.

고정 horizon의 계약은 모든 accepting node의 depth가 n이라는 것이다. 따라서
양수 성공이 있는 이 subtree가 min/max에 기여하는 값은 둘 다 n이다. 변수 horizon에서는
count가 전체여도 실제 선택한 종료 깊이가 달라질 수 있으므로 기존 reconstruction을
유지한다. `collect_patterns`의 취소·storage 오류 처리는 그대로 사용한다. 이 변경은
policy 경로 증거를 새로 생성하는 기능이 아니다. 현재 구현의 `collection[0]`은
`collect_selected` 호출 수, `collection[1]`은 이 전체집합 shortcut 횟수이며 평가마다
초기화한다. `CLEARRA_OBSERVATION_COLLECT_FULL=0/1`로 대조할 수 있고 기본은 1이다.
실제 8-lock EDGE=2에서 collection 호출은 1,560,890에서 11,517로 줄었으며,
전체 subtree shortcut은 10,634회였다. 정책 value 상태를 같은 비율로 줄였다는 뜻은
아니다. 각 arm 한 번의 순차 실행이며 상세 시간·메모리는 별도 측정 문서를 따른다.

### B11. q10: 실제로 관측한 공급 개수로 불가능한 revealed root만 제외

q10은 `d17c26e5f8d411f81c3fc7bd00f22959dbe772b7`에 반영되어 추가 여섯 회귀를
포함한 **92개 observation focused 검증을 통과했다. 실제 P7P4는 93.529510444 s에
공통 work 상한으로 미완료**다. 8-lock ABBA는 원래 coverage가 같고 총시간이 줄었지만
alias와 certificate를 포함한 charged kind work는 늘었다. 자세한 수치는 측정 문서 §3.15다.
처음 fixture의 trie 준비 누락과 수정 뒤 결과는 §9에서 구분한다.
기본 `CLEARRA_OBSERVATION_SUPPLY_MULTISET=0`은 기존 경로이고, mode 1은
`local-search-ab` 실험이다. r≤7 word bitmap을 만드는 §5의 후보와 구분한다. 이 변경은
종류별 개수의 필요조건만 사용하며 단어의 순서나 Hold 가능성을 대신 판정하지 않는다.

#### B11a. 실제 관측으로 구성한 사용 가능 개수의 상계

고정 accepting 깊이를 n, 현재 언어 깊이를 d, 남은 lock 수를 r=n−d, source cursor를 c,
projected lookahead를 포함한 유효 trie 길이를 N이라 하자. 원래 정보집합 I가 실제로
관측한 prefix 길이는 D다. 빈 Hold를 처음 채울 수 있을 때의 추가 draw를

\[
e=\mathbf1[\text{hold enabled}\land\text{hold empty}\land r>0],
\qquad s=\min(c+r+e,N)
\tag{B11a}
\]

로 둔다. evaluator는 terminal supply target이 없고 fixed horizon이 있으며 **D≥s**인
경우에만 공급 count hook을 검토한다. `c>s`, checked arithmetic 실패, 원래 prefix에서
필요한 piece를 읽을 수 없는 경우에는 필터를 쓰지 않는다. `future_is_determined`만
참인 경우에는 기존 unfiltered reveal만 허용한다. 결정된 미래를 실제 관측 배열에 이미
들어 있다고 가정하여 숨은 position의 종류를 세지 않는다.

D≥s이면 모든 q∈I가 q[c..s]에서 같으므로 다음 벡터 A는 I 전체에 공통이다.

\[
A_p=\#\{i:c\le i<s,\ q_i=p\}
  +\mathbf1[\text{hold enabled}\land\text{usable held piece}=p].
\tag{B11b}
\]

여기서 usable held는 코드 1..7이며, 빈 Hold 0·이미 terminal release한 8·disabled Hold는
추가하지 않는다. `revealed_supply_counts`는 실제 원래 trie prefix에서 이 벡터를 만든다.
중복 단어의 원래 ID와 가중치를 변경하지 않는다.

**보조정리 B11a.** 현재 공급 상태에서 완료 가능한 모든 남은 lock 단어 w에 대해
종류별 사용 개수 `Parikh(w)≤A`가 성분별로 성립한다.

Current와 occupied Swap은 draw 하나를 소비하고, 처음 빈 Hold에 Store할 때만 draw
하나를 더 소비한다. 이후 Hold는 swap을 해도 채워져 있고, held release는 종료 전이에만
허용되므로 r locks 동안 읽을 수 있는 원래 draw는 많아야 r+e개다. 남은 lock의 각 미노는
그 draw 중 하나 또는 현재 사용 가능한 held 미노에서 왔다. Hold의 reorder는 같은 미노를
복제하지 않으므로 w의 각 종류 개수는 B11b를 넘을 수 없다. 실제 순서·terminal 조건이
일부 미노 사용을 막으면 A는 더 느슨한 상계일 뿐이며 삭제를 추가로 허가하지 않는다. □

N에는 projected 마지막 draw도 포함한다. `materialized_sequence_len`은 기존 terminal
held-release 권한에 남겨 두며 count의 끝을 그 길이로 잘라 사용 가능한 projected 미노를
누락하지 않는다. 반대로 projected draw를 원래 ID 또는 별도 확률 질량으로 추가하지
않는다. EOF에서 draw가 없더라도 아직 쓸 수 있는 held 하나는 A에 남는다.

#### B11b. 모든 종류 간선에 대한 uniform multiset certificate

하나의 canonical kind root v가 받아들이는 언어를 K(v)라 하자.
`KindUnionArena::uniform_multiset`은 다음 깊이 귀납으로 선택적 증거 M(v)를 만든다.

- accepting·remaining 0·edge 0인 terminal은 M(v)=0이다.
- 비종료 node의 **모든** 간선 `(p,u)`에서 child ID가 감소하고 남은 깊이가 정확히 하나
  줄어드는지 확인한다. 모든 child에 증거가 있고 `e_p+M(u)`가 동일할 때만 그 공통 벡터를
  M(v)로 둔다. 같은 kind의 NFA 간선이 여러 개면 각각 포함한다.
- child 증거가 없거나 서로 다른 multiset을 만들거나 4-bit 성분을 넘으면 unknown이다.
  unknown은 root를 보존하며, malformed ID·깊이·종류는 기존 오류로 처리한다.

이 귀납으로 증거가 존재하면 모든 w∈K(v)가 정확히 같은 Parikh vector M(v)를 가진다.
단어 순서가 같다는 주장은 없다. 예를 들어 두 I 간선의 suffix가 O와 T라면 하나의
간선만 읽어 IO의 개수로 인증하지 않는다. IO와 IT를 함께 가진 root는 unknown으로
남겨 두므로 IT에 맞는 실제 공급에서 잘못 삭제되지 않는다.

증거는 immutable kind arena의 ID에만 유효하다. `bind_kinds`는 해당 `Arc` identity를
고정하고 다른 arena의 같은 숫자 ID를 거절한다. 7개 종류에 4 bits씩 쓰는 count encoding의
범위를 넘으면 더 좁은 증거를 만들어내지 않는다. 실제 공급 count의 encoding 실패는
전체 필터를 건너뛰는 fallback이며, 증거 cache의 선택적 용량 한도는 unknown이다.

#### B11c. 필터 뒤에도 원래 공급 product와 정책이 같은 이유

한 frozen board의 H 경계 suffix roots를 R이라 하고,

\[
R_A=\{v\in R:M(v)\text{가 없거나 }M(v)\le A\}
\tag{B11c}
\]

만 남긴다. 현재 원래 공급 q와 Hold 상태에서 가능한 남은 단어의 집합을 S(q,h)라 하자.
삭제된 root v에는 M(v)>A인 성분이 있으므로 B11a와 certificate에 의해
`K(v)∩S(q,h)=∅`가 모든 q∈I에서 성립한다. 따라서

\[
\left(\bigcup_{v\in R}K(v)\right)\cap S(q,h)
=\left(\bigcup_{v\in R_A}K(v)\right)\cap S(q,h),
\qquad q\in I.
\tag{B11d}
\]

이는 실제 관측으로 고정된 I의 모든 공급·Hold 경로에 대한 동치이며, 공급 밖의 단어까지
포함하는 raw kind 언어가 같다는 주장이 아니다. 남겨진 root도 개수만 맞으면 수용하는
것이 아니라 기존 ordered supply product를 끝까지 통과해야 한다.

H 이전 물리 action은 변경하지 않고, H에서 어느 실제 q에도 성공할 수 없는 suffix만
제외한다. 따라서 해당 frontier의 모든 선택 값이 보존되고, reveal별 합과 물리 action의
max를 위로 귀납하면 원래 root 정책 값도 같다. 실제 관련 draw가 고정된 suffix의 성공은
이 정보집합 전체에 대해 동일하므로 원래 ordinal coverage도 보존된다. weight를 종류
개수로 대체하거나 중복 원래 ID를 하나로 합치지 않는다. 기존 semantic physical tie가
유지되며, 예전 numeric child ID가 선택한 concrete witness까지 같다는 주장은 추가하지
않는다. CountAll·finesse·score·trace 증거로 이 Boolean 필터의 권한을 넓히지 않는다.

이 필요조건 자체는 기존 fixed-depth kind 언어에 대한 논증이다. whole-board producer가
원래 검색과 같다는 별도의 B5 완료·공급 상대 완전성 권한을 만들어내지는 않는다. 현재
제품 연결은 기존 `BoardObservationPlan`의 CoverageSummary/fresh-source/제약 없는 PC
범위와 내부 전체 생산·freeze 완료 gate를 그대로 상속한다.

#### B11d. alias·빈 결과·자원 fallback의 소유 계약

`wants_revealed_supply`는 union mode가 켜진 raw board의 정확한 H 경계, `r>0`에서만
count 추출을 요청한다. 이미 종류/union reference로 바뀐 node, terminal, H 이전 상태와
kind union off는 기존 경로다. terminal supply target이 있거나 variable horizon이면
evaluator가 count hook을 호출하지 않는다.

filtered alias key는 `(frozen board ID, packed A)`다. 같은 frozen view의 board ID가
remaining depth와 immutable root 목록을 정하므로 이 두 값을 다시 key에 넣을 필요가
없다. 숫자 board ID만 다른 owner에서 재사용하거나 root 목록을 바꾸는 용도로 이 cache를
사용할 수 없다. cache hit는 remaining depth도 확인한다. 원래 unfiltered board alias
배열과 별도 map을 사용하므로 한 공급에서 거절된 root 집합을 다른 공급이나 unfiltered
reveal에 덮어쓰지 않는다. policy memo의 cursor·Hold·visible/future class는 그대로이며
종류 count만으로 일반 공급 상태를 합치지 않는다.

필터가 모든 root를 제외하면 같은 remaining depth의 accepting=false, edge 0인
명시적인 reject 언어를 만든다. 이는 위 B11d로 증명된 빈 product다. missing producer를
뜻하는 일반 `intern([])`는 기존 오류를 유지한다. singleton은 기존 kind namespace,
여러 root 또는 명시적 reject는 별도 union namespace를 사용한다. 원래 board/tag ID와
충돌하지 않으며, hash interning은 remaining과 전체 root 목록을 비교한다.

alias 용량 한도나 available count encoding 실패는 unfiltered reveal로 돌아간다.
uniform certificate 용량 한도는 해당 root를 유지한다. 실제 allocation 실패·취소·work
초과·arena mismatch는 오류와 failed-owner latch를 유지하며 확률 0으로 바꾸지 않는다.
선택적 cache에 넣지 못했다는 이유로 unknown을 empty로 해석하지 않는다.

공급 context/alias lookup은 cache hit에도 work 1을 청구한다. 신규 root 검사, certificate
node 조회와 각 edge 방문, 후속 union 작업은 기존 base+union의 **같은 64M work 상한**에
합산된다. cache의 논리적 capacity와 재할당 시 공존 bytes도 owner retained/peak에
반영하며, 이 추정값을 실제 allocator 전체 또는 process RSS와 동일하게 취급하지 않는다.
이 work counter는 명시된 연산 단위이지 모든 CPU instruction이나 시간의 동일성 보장은
아니다. `checked_roots`, `pruned_roots`, `certificate_nodes`, `certificate_edges`,
`aliases` 계측과 전체 work·time·peak를 함께 확인해야 한다. q9의 남은 정책 예산이 작았으므로
prune 수만 보고 필터가 더 싸거나 P7P4를 완주한다고 결론내릴 수 없다.

### B12. q11: 정확한 operation 순서별 source-cap prefix word mask

`queue_observation_prefix_masks.rs`의 bounded helper와
`queue_observation_board_actions.rs::prefix_mask_plan`/`register_inner`의 제품 연결,
worker merge 및 결과 계측까지 작성하고 소스 읽기 감사를 마쳤다. **실행 검증과
실제 비용 비교는 아직 하지 않았다.** `CLEARRA_OBSERVATION_PREFIX_MASK=0`이
기본이며 mode 1은 `local-search-ab` 실험이다. 새로운 source pruning 증거를
만들지 않고, B7이 이미 인증한 같은 cap 검사를 word 단위로 재사용한다.

#### B12a. word mask와 scalar 판정의 필요충분 동치

고정 owner의 연산 수를 n≤11, hidden horizon을 H, 종류 k의 상한을 b_k라 하자.
정확한 operation 종류 순서에서 종류 k인 위치의 비트 집합을 G_k로 둔다.
prefix S의 정수 표현을 `S=(w<<6)|l`, 0≤l<64로 나누고 다음 상수 표를 정의한다.

\[
T[g][t]_l=\mathbf1[\operatorname{popcount}(l\mathbin{\&}g)\le t],
\qquad 0\le g<64,\quad 0\le t\le6.
\tag{B12a}
\]

깊이 mask H_w는 유효한 n-bit prefix 중 `popcount(S)≤H`인 lane만 가진다.
상위 operation 비트의 종류별 개수를
`h_k=popcount(w & (G_k>>6))`라 하자. 어느 h_k>b_k이면 V_w=0이고,
그렇지 않으면 다음 식으로 valid mask를 만든다.

\[
V_w=H_w\ \cap\!\bigcap_k
T[G_k\mathbin{\&}63][\min(6,b_k-h_k)].
\tag{B12b}
\]

상위 비트와 하위 여섯 비트는 서로소이므로
`count_k(S)=h_k+popcount(l & G_k)`다. 따라서 식 B12b의 lane은 정확히
`|S|≤H ∧ ∀k count_k(S)≤b_k`일 때에만 1이다. 전체 operation 개수가 이미 b_k
이하인 종류의 교차를 생략해도 모든 subset이 그 조건을 만족하므로 식은 같다.
n<6에서는 n-bit 범위 밖 lane을 H_w에서 제외한다. 표 T는 compile-time 상수이며
요청마다 2^n개의 scalar 종류 검사를 반복하지 않는다.

`cap_invalid=H_w & !V_w`로 두면 원래 깊이 검사에서 제외되던 prefix는 cap-pruned로
잘못 세지 않는다. `PrefixMaskWord::filter`는 `active & V_w`와
`popcount(active & cap_invalid)`를 반환한다. `cap_invalid=0`인 word는 H 이하의
모든 prefix가 cap-valid이므로 기존 깊이 검사만 하면 된다. 현재 연결은 active가
비어 있지 않고 해당 word에 가능한 cap 위반 lane이 있을 때만 실제 word 필터를
수행한다. active가 그 위반 lane을 포함하지 않는 경우까지 항상 필터 비용을 피한다는
주장은 하지 않는다.

#### B12b. 생산 관계·경로·공급 계약의 보존

mask 계획이 있는 경우 `register_inner`는 개별 prefix의 scalar cap 검사를 생략한다.
위 정리에 의해 masked word와 cap-vacuous word 모두 scalar와 같은 깊이·cap 조건을
통과한다. 살아남은 prefix를 방문하는 순서는 기존 숫자 mask 역순과 동일하다.
H 경계의 retained prefix가 같고, H보다 낮은 깊이에서 retained child가 존재하는지에
대한 역방향 귀납으로 productive prefix도 동일하다. geometry frame, board interning,
typed physical edge, q9의 층 선택 및 H suffix binding은 같은 prefix에서 그대로
수행되므로 같은 후보 순서 아래 보유 graph와 suffix 집합이 보존된다.

이 동치는 B7의 원래 Ω 인증을 새로 부여하지 않는다. `prefix_kind_limits`가 없으면
helper를 호출하지 않으며, source/hold/EOF·B5 완료 권한·reveal 전 물리 행동·q10의
실제 관측 count 경계는 변경하지 않는다. 같은 완료 입력의 정책 값·원래 selected ID와
가중 질량 보존은 기존 graph/product 동치에서 따른다. 작업량 차이로 자원 상한에
도달하는 시점까지 같아야 한다는 주장은 아니다.

#### B12c. exact key, owner와 부분 cache fallback

cache owner는 `(n,H,[b_k;7])`를 고정한다. key는 **정확한 piece sequence**의 각 종류를
3 bits에 넣고 별도 길이 bits를 둔다. n≤11에서 sequence와 길이 영역은 겹치지 않으며,
HashMap은 이 full key의 equality를 확인한다. catalog의 현재 kind-blocked 정렬은
필수 전제로 삼지 않는다. `[I,I,O]`와 `[I,O,I]`는 같은 multiset이지만 `I≤1`일 때
prefix `011`의 허용 여부가 다르므로 서로 다른 entry다.

entry는 append-only이고 `PrefixMaskPlan` ticket은 slot·exact key·owner scope를
보유한다. `word`는 이 세 값을 확인하며 hit 때 32-word 배열을 복사하지 않는다.
owner는 하나의 공통 H mask를 보유하고 entry에는 V_w와 cap-sensitive word bitmap만
저장한다. 512-entry 상한 뒤 새 key가 오면 lookup 비용을 기록한 뒤 `plan=None`을
반환한다. 이미 저장한 key의 hit는 계속 유효하다. 이 **부분 cache** 상태에서도 각
candidate는 exact 계획 또는 기존 scalar cap 검사 중 하나를 온전히 사용하며,
unknown mask를 모든 prefix 허용으로 해석하지 않는다. n>11, cap 미인증, mode 0도
기존 scalar 경로다. allocation·취소·work 오류는 fallback으로 숨기지 않는다.

worker별 cache는 독립이며 같은 `BoardObservationPlan` Arc 아래에서도 cache entry나
ticket을 다른 worker에서 그대로 사용하지 않는다. `merge_inner`는 실제 생산된
board/edge/suffix와 계측만 합치고 incoming cache는 병합이 끝나면 버린다. 따라서
다른 slot 번호나 cache admission 순서가 검색 의미에 영향을 주지 않는다.

#### B12d. 사전 work 청구, 취소와 메모리 상한

`prefix_mask_plan`은 helper에 `MAX_WORK−owner.work`를 전달한다. helper는 bounded
key lookup, common horizon 생성, miss의 piece/group/word 계산, 임시 32-word 초기화,
보유 32-word 복사와 insertion을 **실행 전에** 해당 예산에서 청구한다. 성공 때만
검사하는 사후 상한이 아니다. cache hit에도 lookup 1을 청구하고 full-cache fallback도
이 비용을 남긴다. 호출자는 반환값의 성공 여부와 무관하게 helper 누적 work의 delta를
checked-add로 기존 owner의 같은 64M 상한에 정확히 한 번 반영한다. 취소 도중 이미
수행한 작업도 계상하며, register 오류는 family를 failed 상태로 만든다. 부분 cache나
부분 등록을 정상적인 empty family 또는 확률 0으로 전달하지 않는다.

실제 word 필터에는 먼저 `RUNTIME_WORD_WORK=1`을 청구한다. 남은 개별 prefix와
productive/edge 등록은 기존 청구를 유지한다. `[lookups,hits,misses,full_fallbacks,
words,work]`의 마지막 work는 helper와 word 필터의 추가 비용이며 전체 board work에
이미 포함된다. merge가 incoming 전체 work를 더한 뒤 세부 계측을 합칠 때 다시
청구하지 않는다. 이 work는 명시된 bounded 연산 단위이며 CPU instruction 수나
wall time과 동일하다는 주장은 아니다.

entry당 mask payload는 최대 32 u64=256 bytes, 512 entries의 payload는 128 KiB다.
exact key·slot index·word bitmap·구조체 padding·HashMap capacity는 이 payload 밖의
실제 owner 추정에 별도로 포함된다. common horizon 배열은 cache owner 안에 한 번,
3,584-byte 표 T는 executable의 공유 상수에 한 번 존재한다. 첫 insertion에서 bounded
entry/index capacity를 reserve하여 이후 hit마다 큰 배열을 복사하지 않는다.
`retained_bytes`는 heap capacity, `peak_bytes`는 성장 시 old/new buffer의 공존분을
기록한다. board는 cache owner Vec와 heap을 각각 한 번 포함하고 helper의
`peak−retained`를 보태며, worker cache의 incoming bytes도 merge 중에 포함한다.
stack의 짧은 임시 배열과 실제 allocator metadata까지 이 heap 추정으로 측정했다고
해석하지 않는다.

#### B12e. 실제 작업량 비교식과 검증 상태

같은 입력·후보 순서·merge 순서에서 mask 없이 cap 위반으로 scalar 방문한 prefix 중
마스크가 방문 전에 제외한 수를 R, helper query의 전체 비용을 Q, 실제 filtered word
수를 W라 하자. 다른 생산 연산은 위 동치에 의해 같으므로 다음이 성립한다.

\[
\mathrm{work}_{11}-\mathrm{work}_{\mathrm{scalar}}=-R+Q+W.
\tag{B12c}
\]

cache miss·lookup·초기화·복사 비용을 빼고 R만큼 work가 줄었다고 보고하지 않는다.
`source_pruned_prefixes`는 popcount로 기존 논리적 제외 수를 보전하고,
`prefix_visits`는 실제 개별 방문이 줄어든 만큼 감소한다. Q+W<R인지, 그 차이가
정책 평가를 남은 상한 안에 넣는지, 총시간과 peak도 나아지는지는 실제 비교가 필요하다.
모든 active word에 무조건 비용을 청구하는 안은 q9 P7P4의 456,459 candidates에서
최대 7,303,344 word 비용과 최소 456,459 lookup을 만들 수 있어, 기존 cap-pruned
5,913,906개만으로도 절감이 보장되지 않는다. 현재 조건부 word 처리의 실제 W를
측정하기 전에는 완주나 속도 개선을 확정하지 않는다.

helper에는 표 T 전수 대조, n=1..11과 모든 H의 scalar 대조, 중복 종류·held 상한 2,
같은 multiset의 순서 차이, 512-entry 이후 fallback, 사전 예산·취소 및 owner ticket
범위의 7개 테스트가 있다. 등록 on/off·merge·work 오류 후 owner 거절을 포함해
q11의 observation focused 101개를 통과했다. 실제 8-lock ABBA의 전체 coverage도 같지만
평균 시간 개선은 관측하지 못했다. 10-lock 두 모드는 등록·freeze 뒤 work 상한에서
중단되어 실패시간 비율을 만들지 않는다.

### B13. q12: 보유한 suffix의 자식 closure만 coordinator에 import

worker의 종류 DAG를 $D=(V,E)$, 실제 board owner의 suffix root 집합을 $R$라 하자.
필요한 노드 집합은 $C=\operatorname{Reach}_D(R)$다. 어떤 $r\in R$에서 시작한
단어 경로의 모든 노드는 정의상 $C$ 안에 있다. 따라서 $D$를 $C$로 제한한 DAG는
모든 보유 root에 대해 정확히 같은 단어 언어를 갖는다. root가 다른 깊이의 불필요한
oracle 조상이나 다른 연결 성분을 복사할 필요는 없다.

구현은 원래 노드의 child ID가 parent ID보다 작다는 불변식을 검사한다. root를 표시한
뒤 ID 내림차순으로 모든 labelled child를 표시하고, 오름차순으로 기존 exact interner에
넣는다. terminal 노드에서 언어가 같고, 이미 같은 언어로 매핑된 모든 child를 같은
label과 accepting/depth로 연결하므로 위상 순서에 대한 귀납법으로
$L_D(r)=L_{D'}(f(r))$가 모든 $r\in R$에서 성립한다. 같은 종류의 여러 child도 전부
남기므로 NFA의 alternative를 대표 child 하나로 줄이지 않는다. hash가 같다는 이유만으로
노드를 합치지 않고 기존 interner의 exact edge-list 비교를 그대로 사용한다.

`PhysicalActionLanguage::import_board_suffixes`는 frozen 이전 board owner만 받는다.
Small/Large suffix 집합을 모두 순회하고 root의 expected remaining depth를 검사하며,
source의 physical owner와 board owner가 같은 kind arena를 참조하는지 확인한다.
반환 `PieceLanguageSubsetImport`는 source와 destination의 Arc identity를 모두 pin하고
`rebound_for`에서 다시 대조한다. 기존 destination ID는 바뀌지 않는다. 따라서 기존
standard-bag cache나 이미 병합한 board root의 ID를 바꾸는 작업이 필요하지 않다.

오류·취소 시 완성되지 않은 mapping은 반환하지 않는다. 이미 append한 destination
노드는 정상 interner가 만든 노드이므로 기존 언어를 바꾸지 않으며, mapping을 받지 못한
physical shard는 연결되지 않는다. 정상일 때는 기존 `merge_factored`가 root를 한 번
재바인딩한다. 원래 merge의 physical work를 유지하고 추가 GC 순회를 숨기지 않는다.

이 구현은 dense mapping의 초기화·표시 scan·import scan을 사용한다. 비용은 단순히
$O(|C|)$가 아니라 $O(|V|+|R|+|E_C|)$에 exact interning 비용을 더한 것이며,
mapping 메모리도 $4|V|$ bytes다. `mapping_slots`, root/node/edge visits를 따로 보고한다.
노드·간선 계측은 구조 방문이며 hash bucket 비교 전체의 CPU 명령 수를 뜻하지 않는다.
byte 계측은 source+destination+map과 buffer 성장 시 old/new 공존을 포함한다.
결과의 worker aggregate와 destination owner가 이미 보유하는 양에 map·일시 성장분만
추가하여 double count를 피한다. 여러 shard의 count는 합산하고 byte snapshot은 최대값을
사용한다. 기존 physical/kind 64M 상한의 정의나 누적값은 바꾸지 않는다.

`CLEARRA_OBSERVATION_SUFFIX_IMPORT=0|1`은 local A/B 옵션이며 기본 0이다. factored
owner는 원래 complete import를 유지한다. worker가 생산 중 가진 원본 arena를 줄이는
GC는 이 변경에 포함하지 않는다. 원본 arena가 256MiB recycle 판단을 계속 넘는 문제도
여전히 별도로 남는다. q12의 5개 회귀는 실제 coverage/원래 ID·depth·physical work,
same-kind child, 두 owner epoch, malformed 부분 결과·취소·freeze 경계를 검증했다.

### B14. q13: 관측 변환 전에 같은 정확한 DP 값을 재사용

기존 recurrence가 raw 상태 $s$를 실제 관측에 따른 suffix union $N(s)$로 변환한 뒤
$V(N(s))$를 계산한다고 하자. q13은 이 변환을 요구하는 raw board 상태에만 기존의
정확한 `PolicyMemoState` key $K(s)$를 사용하여 완성된 값 $(score,pattern\_count)$를
보관한다. key에는 language class, 원래 trie의 future class, 실제 visible window,
source cursor, Hold/EOF 코드 및 terminal target이 요구하는 consumed-bag 상태가 있다.
이는 multiset 하나만으로 물리 상태나 보이지 않는 미래를 합치는 변경이 아니다.
visible length는 별도 필드가 아니어도 각 piece code가 1..=7이고 남는 3-bit slot은
0이므로 visible-window 정수가 길이까지 단사적으로 보존한다. future class는 단순한
종류별 개수가 아니라 종류별 child class, subtree count, 비균등 subtree weight의
정확한 f64 bits를 포함한다. 같은 key에서 남은 관측 길이·EOF·relevant-end와 사용
가능한 종류 count를 복구하는 데 필요한 정보가 사라지지 않는다.

생산 Board view의 raw memo class는 같은 board node ID다. 같은 raw key는 같은
depth/cursor/실제 관측 길이/hold와 같은 relevant-end, count 및 determined 분기를
만든다. q10 alias는 저장한 key를 먼저 조회하고 해당 view 안에서 삭제하지 않으므로
같은 normalization 결과의 numeric ID가 유지된다. 따라서 직접적으로
$K(s)=K(t)\Rightarrow K(N(s))=K(N(t))$이며, 기존 canonical memo가 반환할
`PolicyValue`의 score bits와 count를 그대로 재사용한다. 이는 기존 부동소수점
recurrence와 memo의 결과 동치이며, 일반 가중 합의 재계산 순서 독립성을 새로 가정하지
않는다. uniform 입력의 정수 count 처리도 그대로다.

miss는 기존 normalization과 canonical memo를 끝까지 실행한다. hit는 그 호출 자체를
생략한다. already-normalized kind/union 노드와 reusable language에는 적용하지 않는다.
오류나 취소를 reject 값으로 저장하지 않는다. 평가 시작, 각 initial observation,
fork 및 target/domain 전환에서 raw 값을 비우므로 arena/domain을 넘는 재사용도 없다.
최대 65,536개 후에는 새 entry를 넣지 않고 기존 recurrence를 실행한다.

`collect_selected`의 normalization·action replay는 변경하지 않는다. 따라서 선택한 원래
ID, 비균등/0 가중치 duplicate, physical action의 semantic tie, terminal target 처리와
q8 full-subtree 수집 조건도 그대로다. 이 값 cache는 raw 값을 행동 경로나 원본 ID로
해석하지 않는다.

새 DP lookup/hit/insert/full-fallback은 별도 지표이며 기존 generic DP memo와 같은
계상 영역이다. q10 alias lookup을 수행했다가 청구를 취소하지 않는다. hit에서 실행하지
않은 변환만 사라지므로 실제 language work가 줄어든다. 64M 상한이나 기존 query의
charge 단위는 그대로다. cache retained bytes와 old/new table 공존 peak를 보고하며,
현재 owner에 이미 포함된 retained를 다시 더하지 않고 추가 peak분만 반영한다.
`CLEARRA_OBSERVATION_PRE_REVEAL_MEMO=0|1`의 기본은 0이다. 7개 추가 회귀는 실제
normalization 호출 생략, original IDs/가중치, hidden 관측, Hold/EOF/projected draw,
target/domain·scope, 실패·취소와 capacity fallback을 다룬다.

실제 q13b 8-lock ABBA에서는 평균 10.3844852355→11.5150504765초로 악화되었다.
21,160,524 probes 중 9,874 hits여서 실제 alias work 절감은 작았다. P7P4는 65,536개
admission cap과 공통 work 상한에서 여전히 미완료다. 그러므로 기본 off를 유지한다.
취소/오류를 reject로 저장하지 않는 계약은 보존하지만, 작업 절감으로 자원 한도나
비동기 취소를 어느 wall-clock 시점에 만나는지까지 같다는 뜻은 아니다.

### B15. q14: fresh-bag 후보의 존재 여부를 oracle product 없이 판정

이 절의 추가 전제는 **실제 원본 전체 fresh Standard7Bag**, $7\le n\le11$ locks,
실제 materialized 길이 $N=n+1$, cursor 0, 처음에 빈 Hold, Hold 허용, terminal supply
target 없음, 두 lookahead projection 모두 false다. 기존 board closure의 전체 family·
CountUnique/CoverageSummary·evidence/score/filter 배제도 유지한다. request factory가
실제 공급 저장소와 cardinality/양의 uniform 질량을 검증한 뒤 같은 Plan Arc에 권한을
저장한다. 단순 queue 이름이나 후보별 성공 집합으로 권한을 만들지 않는다.

완성 물리 경로의 종류 단어를 $w=w_1\cdots w_n$이라 할 때 다음이 성립한다.

$$
\exists q\in\Omega:\operatorname{HoldExecute}(q,w)
\iff
\left|\{w_1,\ldots,w_6\}\right|=6
\quad\land\quad
\forall p,\ \operatorname{count}_p(w)\le2.
$$

**필요성.** 빈 Hold를 처음 저장할 때만 한 lock에 두 draw를 쓰고 이후에는 한 draw를
쓴다. 첫 여섯 lock까지 소비할 수 있는 draw는 최대 일곱 개여서 모두 첫 bag에서 온다.
따라서 그 여섯 종류는 다르다. $N\le12<14$이므로 원래 공급에는 종류마다 최대 두 개만
있고, 초기 held가 없으므로 전체 배치에도 종류별 2개 상한이 필요하다.

**충분성.** 첫 여섯 종류를 $a_1,\ldots,a_6$, 빠진 종류를 $h$라 하자. 첫 bag을
$h,a_1,\ldots,a_6$로 두고 첫 lock에서 $h$를 저장한 뒤 $a_1$을 놓는다. 이후
current를 사용하면 여섯 lock 뒤 cursor는 7이고 Hold는 $h$다. 남은 단어 $r$의 길이
$m=n-6$은 1..5다.

1. $r$에 $h$가 없으면 종류별 2개 상한에 의해 $r$의 모든 종류가 서로 다르다.
   두 번째 bag의 앞 $m$개를 $r$로 두고 current를 사용하면 된다.
2. $r$에 $h$가 있으면 첫 번째 $h$ 하나를 삭제해 $u$를 만든다. 처음 여섯 종류는
   $r$에서 각각 최대 한 번, $h$는 최대 두 번이므로 $u$는 distinct word다.
   $u$와 $h$에 없는 종류 $f$를 고를 수 있다. 제외되는 종류가 최대 다섯 개이기
   때문이다. 두 번째 bag의 앞 $m$개를 $u f$로 둔다. 삭제한 $h$의 위치 전까지
   current를 사용하고 그 위치부터 Hold swap을 이어가면 $r$이 정확히 복원된다.
   실제 automaton은 같은 종류의 `SwapHeld`도 허용하므로 연속된 $h,h$도 포함된다.

두 구성 모두 끝까지 current가 존재하고 정확히 $N$ draw를 소비하며 held 하나를
남긴다. EOF held-release 특례가 필요하지 않다. 반면 projected-unplaced acceptance는
`cursor=N, held=Some`을 수용하지 않으므로 그 모드는 명시적으로 제외한다.

한 candidate $C$의 전체 종류별 개수는 실행 순서와 무관하다. 원래 exact physical
relation의 `active`는 root-reachable과 terminal-productive의 교집합이다. 따라서

$$
\operatorname{OraclePositive}(C)
\iff
\bigl[\forall p,\operatorname{count}_p(C)\le2\bigr]
\land
\exists S:\operatorname{active}(S)\land |S|=6
\land|\operatorname{kinds}(S)|=6.
$$

오른쪽의 $S$에 이르는 경로는 $S$의 여섯 operation만 사용하므로 순서가 어떻든 모두
distinct다. productive 조건으로 terminal까지 잇고 위 충분성 구성을 적용할 수 있다.
역방향은 oracle 성공 경로의 여섯-lock prefix가 직접 witness다. 이 판정은 현재
oracle-positive candidate 집합을 유지한다. 단지 물리 언어가 비지 않았다는 이유로
source-infeasible 후보까지 등록하는 더 느슨한 변경과 구분한다.

q14는 typed `ObservationProducerDecision::{NotSelected,Rejected,Registered}`를 쓴다.
등록 경로는 oracle coverage row/질량/최소 witness ID/실행수를 반환하는 척하지 않는다.
관측 producer에 필요한 physical relation과 H-depth suffix만 전달하고, 최종 정책은
기존 whole-family gate 뒤 원래 Ω에서 계산한다. geometry ordinal과 memory sampling은
매 candidate에서 유지한다. 기존에도 경로별로 달라질 수 있었던 작업 진단값인
`coverage_row_count`와 `pattern_verified_execution_count`는 실제 생략된 oracle 단계에
대해 0이다. candidate 생산·거절·인증·prefix 검사·suffix edge 방문은 별도로 보고한다.
per-solution rows/trace/score/CountAll 및 serial/general 호출은 기존 경로에 남는다.

종류 compiler는 기존 exact bottom-up interner를 공유하면서 depth $<H$의 조상만
생략한다. terminal에서 H까지의 모든 labelled child가 같으므로 위상 깊이에 대한
귀납법으로 모든 필요한 H root의 언어는 원래와 같다. raw root0가 없는 정상 결과는
typed `Compiled`이고 실제 empty relation은 `EmptyRelation`이다. 둘을 `None`으로
섞어서 성공 후보를 음성으로 바꾸지 않는다. 원래 임의 종류 언어 전체의 root0나 oracle
coverage가 필요하면 기존 compiler를 사용한다.

q14는 `188e583b7e59767d3f95d88ad430585d26cf0d75`에 통합했다. 관측 경로 120개와
H-only suffix 3개 검증을 통과했고, 실제 automaton으로 구성한 35,126개 단어와 별도
bag product 경계 대조를 포함한다. 이 실행 검증은 위 전칭 증명을 대신하지 않는다.
strict-prefix A/B 및 operation-mask A/B 요청은 기존 evidence 경로를 유지한다.
같은 binary의 8-lock ABBA는 coverage가 같고 평균 시간은 10.606760923초에서
10.979079764초로 늘었다. 10-lock은 전체 후보 생산·freeze 뒤 64M work 한도,
11-lock은 578.629917026초에 physical node 한도로 미완료다. 11-lock receipt는 정상
오류 반환 exit 1이며 timeout, SIGKILL 또는 OOM으로 표시하지 않는다.

### B16. q15: 첫 여섯 distinct 조건으로 물리 경로 자체를 축약

추가 권한은 B15와 같고, board horizon은 $H\le5$다. 원래 labelled prefix DAG를
$G_C=(V_C,E_C)$, 여섯-lock 경계 중 distinct 종류를 가진 subset 집합을 $D_C$라 하자.
깊이 6의 $V_C\setminus D_C$만 차단하고, 양 끝점이 허용된 원래 간선만 남긴 그래프에서
root-reachable과 terminal-productive의 교집합 $V'_C$를 다시 계산한다.

모든 완성 경로는 깊이 6을 정확히 한 번 지난다. 깊이 6 이하에서 같은 종류를 반복한
prefix는 여섯 개의 distinct 종류로 자랄 수 없다. 그러므로 이 구성은 깊이 6 이하의
중복-kind prefix를 전부 차단하는 것과 같다. 앞쪽의 막다른 경로와 뒤쪽의 root에서
도달할 수 없는 경로도 함께 사라진다. 단순 `active &= allowed`로는 이 두 조건을
보장할 수 없다.

**원래 공급에 대한 모든 실행 경로 보존.** 임의의 원래 $q\in\Omega$와 candidate
$C$의 성공한 물리·Hold 경로를 고르자. B15의 필요성으로 그 첫 여섯 종류는 모두 다르다.
따라서 그 경로의 depth-6 subset은 $D_C$에 있고 모든 간선이 허용 그래프에 남는다.
이 경로 자체가 각 prefix의 reachability와 productivity를 증명하므로 closure에서도
삭제되지 않는다. 역방향은 원래 간선을 추가하거나 수정하지 않았다는 사실에서 즉시
성립한다. 즉 $L(G'_C)\cap A_q=L(G_C)\cap A_q$가 모든 원래 $q$에 대해 성립한다.
여기서 $A_q$는 원래 Hold/EOF 공급 automaton이며, 임의의 raw 종류 단어 전체에서
$L(G'_C)=L(G_C)$라고 주장하지 않는다.

**Board merge와 조기 reveal.** 여러 candidate의 prefix와 suffix를 board 상태로
이어 붙인 실행도 B5의 root-product sandwich를 따른다. 원래 공급에서 성공하는
실제 전체 실행은 B5의 catalog lift와 full-family 전제 아래 전체 cover를 가진다.
그 전체 실행은 위 필요조건을 만족하므로 해당 cover의 전체 경로가 $G'_C$에 남는다.
따라서 제거된 다른 이력에 속한 suffix가 우연히 필요해 보이는 경우에도 같은 실제
실행의 전체 cover가 witness를 제공한다. 물리 간선의 soundness, 원래 공급 automaton,
전체 family 완료 gate를 함께 유지하면 숨은 정책의 root 값과 원래 ID coverage가 같다.
전체 생산이 끝나지 않은 상태에는 이 결론을 적용하지 않는다.

구현은 `SourceQualifiedPrefixOrders<'a>`가 같은 immutable `SymbolicPrefixOrders`의
물리 proof·geometry와 `SourceQualifiedPrefixMask<'a>`를 함께 borrow한다. 원래
`ExactPrefixRelation`은 변경하지 않는다. suffix compiler와 board 등록은 이 **같은
typed view**를 소비하며, 원래 owner의 pointer identity를 대조한다. child 조회도
filtered active mask를 확인하므로 원래 live derivative가 제거된 child를 되살리지 않는다.
owned `produce_board_relation` 한 호출 안에서 view 생성·suffix compile·등록을 끝내고
positional scratch를 지워 다른 candidate가 재사용하지 못한다.

**중복 검사 제거.** $H\le5$인 보유 prefix는 distinct 여섯 종류 경계까지 이어지므로
이미 각 종류를 최대 한 번만 쓴다. 따라서 B7 scalar cap 및 B12 prefix word mask보다
강한 증거가 있다. typed source 등록에서만 두 계산을 생략한다. raw 등록은 plan에
flag가 있더라도 원래 cap을 수행한다. 수행하지 않은 cap 계산의 work는 발생하지 않으며,
기존 physical work의 단위·누적값·64M 상한을 바꾸거나 리셋하지 않는다.

새 필터는 heap allocation 없이 기존 `PrefixDomain::live_from_start`를 재사용한다.
$W=\lceil2^n/64\rceil$일 때 allowed/parent/검증 mask 방문은 정상 완료에서
$W(n+2)$, distinct boundary 검사는 최대 $\binom n6\le462$다. 결과의
`source_filter=[calls,mask_words,domain_words,prefix_checks]`는 실제 방문을 기록하고
취소·오류 직전의 작업도 남긴다. `domain_words`는 기존 closure의 nonzero transfer-word
정의다. 이 값은 source compiler의 구조 계측이며 전체 CPU 명령 수나 physical owner의
work와 같은 값이 아니다. 전체 시간 비교는 이 추가 생성 비용까지 포함한다.

`CLEARRA_OBSERVATION_SOURCE_FILTER=0|1`의 기본은 0이며 B15 권한이 없으면 적용하지
않는다. `55167107eee0fa66a880a355b430cb45f5d89979`에서 관측 124개 및 직접 필터
7개 검증을 통과했다. 실제 10-lock geometry의 full/H4 모든 보유 suffix를 독립적인
raw-word 종류 필터와 대조하고, weighted duplicate의 원래 ID와 worker merge,
typed/raw 등록의 cap 비용, 오류·취소·owner 경계를 확인했다. 같은 binary의 8-lock
ABBA는 coverage가 같고 평균 10.7221212375→10.019080470999999초였다. 10-lock의
physical work는 50,068,673으로 줄었지만 정책 work 13,931,327에서 공통 64M에
도달했다. 11-lock은 아홉 worker 생산이 끝난 뒤 coordinator 병합의 node 한도에서
미완료다. q15 물리 생산을 유지하는 다른 정책 최적화가 이 병목을 해소한다는 뜻은 아니다.

### B17. q16: 고정 보드의 suffix를 정확한 종류 개수로 먼저 묶기

B11은 실제 관측한 공급 개수 $A$가 주어질 때, 같은 frozen board의 root 목록을
다시 훑어 필요한 종류 개수 $M$이 $M\le A$인 root와 아직 증명하지 못한 root를
남긴다. 새로운 $(board,A)$마다 이 순회를 반복할 필요는 없다. 기존 exact
`uniform_multiset` certificate를 사용해 한 보드의 root들을 $G_M$으로 묶고,
unknown root 집합 $U$를 별도로 보유할 수 있다. Unknown을 불가능으로 해석하지 않는다.

모든 root의 고정 잔여 길이가 $r$이므로 인증된 $M$은 $|M|=r$이다. 실제 relevant-end
관측에서 available count는 $|A|\le r+1$이다. 따라서 fitting multiset의 집합은

$$
K(A,r)=
\begin{cases}
\varnothing & |A|<r,\\
\{A\} & |A|=r,\\
\{A-e_p:A_p>0\} & |A|=r+1.
\end{cases}
$$

마지막 줄의 원소 수는 최대 일곱 개다. 종류 수 일곱이라는 사실만 이용하며 각 종류의
배열 순서나 미공개 미래를 가정하지 않는다. 그룹을 사용하는 결과는
$U\cup\bigcup_{M\in K(A,r)}G_M$로, B11 scalar 판정이 남기는 root 집합과 정확히 같다.
원래 NFA의 모든 same-kind child, accepting/depth, Hold/EOF 공급 automaton은 그대로다.
Group index는 Boolean 결과나 oracle coverage를 저장하지 않는다.

구현 범위는 같은 pinned kind arena와 frozen board ID에 속한 optional index다.
원래 root ID를 변경하지 않으며 선택한 root들을 기존 exact union interner로 전달한다.
direct API의 더 큰 available multiset, optional cache 한도, unknown certificate 등은
기존 q10 처리로 되돌아갈 수 있다. Index를 비우더라도 설치된 kind/union 노드나
supply alias의 ID를 바꾸지 않는다. 보관한 결과를 수정하거나 다른 owner에서 재사용할
권한도 생기지 않는다.

이 변경은 q10의 기존 alias hit당 1 work를 없애지 않는다. 절감 대상은 **새 supply
context에서 같은 보드의 전체 root를 반복해서 순회하고 certificate를 조회하는 비용**이다.
Index 구축·key 조회·실제로 읽은 root·eviction 비용은 기존 kind-union work에 청구하며
64M 누적 상한을 그대로 적용한다. 추가로 보유한 root reference도 base/union/group
합산 8M 아래에 둔다. optional index 때문에 필수 union 저장이 막히면 index를 비우고
기존 저장 경로를 유지한다. 이때 이미 수행한 work는 되돌리지 않는다.

`multiset_group=[builds,build_root_visits,key_lookups,matched_root_visits,unknown_root_visits,
fallbacks,retained_members,work,evictions]`는 이 추가 구조의 계측이다. `work`는 이미
기존 kind-union work에 포함된 부분량이므로 보고서 합계에 다시 더하지 않는다.
retained/peak는 HashMap·Vec capacity 및 성장 시 old/new 공존을 포함한다.
`CLEARRA_OBSERVATION_MULTISET_GROUPS=0|1`은 기본 0인 A/B 축이며 실제 관측 공급 필터와
lazy kind union이 켜진 경로에서만 사용한다. `86aa5b8e255c53636bfacc21a04c4efa706c2759`에
통합했고 새 group 회귀 다섯 개와 실제 board의 세 모드 원래 ID 대조를 포함한 관측
129개 검증을 통과했다. 실제 성능 비교는 진행 중이다.

### B18. q17: 원래 정보집합의 성공 개수 상계로 불리한 행동 계산을 중단

한 정보집합 $I$에서 행동 $a$를 실행한 뒤 드러나는 관측 분기들을
$I_{a,1},\ldots,I_{a,m}$이라 하자. 기존 trie 전이는 이 집합들을 서로 겹치지 않는
원래 terminal ID 집합으로 나누며 합집합은 $I$다. 각 분기에서 이미 계산한 정확한
최대 성공 개수를 $v_j$, 원래 subtree 전체 개수를 $c_j$라 하면 $0\le v_j\le c_j$다.
처음 $k$개 분기를 계산한 시점의

$$
U_k(a)=\sum_{j\le k}v_j+\sum_{j>k}c_j
$$

는 이 행동이 달성할 수 있는 모든 후속 관측 정책의 성공 개수 상계다. 아직 계산하지
않은 분기가 전부 성공한다고 가정하기 때문이다. 다른 행동 $a^*$에서 이미 **정확히
계산한** 값 $B$를 얻었다면 다음 경우 $a$의 남은 계산을 생략할 수 있다.

- $U_k(a)<B$.
- $U_k(a)=B$이고 기존 `transition.precedes(a*)` 동률 규약에서 $a$가 앞서지 않는다.

같은 값으로도 먼저 선택돼야 할 행동은 계속 계산한다. incumbent가 없을 때는 이
판정을 적용하지 않아 처음 발견한 legal zero-valued 행동을 거절 상태와 섞지 않는다.

첫 구현은 `uniform_weight.is_some()`, fixed horizon, terminal supply target 없음으로
제한한다. 이 모드에서 기존 recurrence의 score는 정확한 정수 pattern count이고
u32 전체 범위는 f64로 정확히 표현된다. 그러므로 실수 가중 합의 반올림 순서를 바꾸는
문제를 끌어오지 않는다. 현재 원래 trie의 uniform certificate는 양의 동일 가중치에서
발급되며, 일반 0 가중치나 비균등 입력은 원래 경로에 남는다. Hold/EOF 및 terminal
release의 전이·acceptance 규약도 변경하지 않는다.

중단 결과는 typed `Dominated`이며 정확한 0, `REJECT`, memo value가 아니다. 이 값을
q13 cache나 기존 canonical memo에 저장하지 않는다. 방문한 child의 solve 값은 모두
정확하며, 부모는 완전히 평가된 incumbent와 정확한 상계 비교로 최대값을 확정한다.
따라서 부모 solve가 저장하는 값은 여전히 정확하다. `collect_selected`가 같은
best-transition 규약으로 재생할 때 winner와 원래 ID 집합도 보존한다.

이 계산은 실제 숨은 미노를 본 것으로 취급하지 않는다. 같은 원래 정보집합 전체의
가능한 후속 관측을 합산하는 기존 정책 DP에서, 그 집합의 공개된 cardinality 상계만
쓴다. 행동을 각 미공개 미래마다 따로 선택하는 권한은 추가하지 않는다.

새 `branch_bound=[attempts,pruned_actions,skipped_observations,checks]`는 DP 단계의
진단이다. 실제 생략한 후속 solve와 kind query만 사라지며 이미 수행한 physical/kind
work를 되돌리거나 상한 정의를 바꾸지 않는다. 오류·취소와 malformed count는 음성
정책으로 저장하지 않는다. q17 frozen binary는 기본 off인 비교 축으로 검증했다.
관측 경로 135개가 통과했으며 실제 8-lock ABBA는 원래 ID별 coverage가 같고
9.7012302315→3.9884741035초였다. 10-lock 두 조합은 work 상한으로 미완료다.
이후 제품은 위 guard를 그대로 유지해 기본 on으로 채택하고,
`CLEARRA_OBSERVATION_BRANCH_BOUND=0`은 local A/B reference로 유지한다.

### B19. q18: 같은 H 배치 집합에 도달하는 전체 물리 경로 공유

**범위.** B16의 typed source-qualified view, 같은 요청·catalog·물리 규칙·초기 보드,
`boundary_suffixes_only`, $1\le H\le5$에 한정한다. raw relation이나 종류 개수만
같은 다른 배치에는 적용하지 않는다. $P$는 active H-prefix를 구성하는 원래 배치
집합이다. B16에 의해 P의 종류는 모두 서로 다르다. 따라서 P의 exact identity는
종류별 원래 배치 셀로 표현할 수 있다. `GeometryCatalog::compile_with_clear_row_domain`
은 같은 `(piece,original cells)`의 모든 rotation·required-deleted-row realization을
하나의 SkeletonRow에 모으며, `skeleton_id`도 그 pair로 조회한다.
`CandidateGeometry`는 특정 pose를 선택하지 않고, symbolic target 판정은 해당
row/frame의 모든 instantiation 결과를 existential OR한다. 따라서 같은 catalog와
entry/kick authority 범위에서는 이 key가 모든 회전 의미도 보존한다. 단순 현재
board equality로 key를 약화하지 않는다. 요청별 family 생성·parallel의 동일
catalog/Plan 공유가 이 authority 수명을 고정하며, Plan 값 자체에 kick hash가
포함된다는 주장은 아니다.

**Cone의 정의.** $C(P)$는 실제 초기 보드에서 정확히 P의 배치를 모두 실행하는
모든 합법 순서에 등장하는 prefix와 물리 간선의 합집합이다. 하나의 성공 순서나
spanning tree가 아니라 모든 predecessor 경로가 들어간다. $S\subseteq P$에서
삭제될 원래 행과 현재 board는 초기 셀과 S의 원래 셀의 합집합으로 정해진다.
그 상태에서 다음 P-operation의 실제 도달성도 같은 board와 같은 target 및
고정된 ordered kick 규칙으로 정해진다. 아직 실행하지 않은 나머지 완성 타일링은
이 물리 사실을 바꾸지 않는다.

**Productivity 독립성.** 현재 후보에서 P가 source-qualified active라는 사실은
P 이후 terminal까지 이어지는 witness $w$가 있다는 뜻이다. 임의의 합법
root→P 순서 $v$를 택하자. P에 도착한 실제 board가 동일하므로 같은 $w$를
이어 붙일 수 있다. 첫 여섯 종류의 distinct 조건은 v의 내부 순서가 아니라
P의 종류 집합과 w의 처음 $6-H$개 종류로 정해진다. 그러므로 $vw$도 B16의
source 조건을 만족한다. 반대로 원래 후보가 P까지 증명한 모든 간선은 실제로
합법인 물리 전이다. 따라서 동일 P를 가진 어떤 source-positive 후보에서도
root→P의 productive cone은 정확히 $C(P)$이고, 남은 배치의 모양이나 suffix
언어 차이에 의존하지 않는다.

**등록 그래프의 equality.** 기존 등록의 깊이≤H active prefix와 간선은 각각
어떤 active H-prefix P에 도달한다. 전체 등록 그래프는 모든 그런 P의
$C(P)$ 합집합이다. 첫 P를 만났을 때 cone 전체를 등록하고 그 완료 사실을
cache certificate로 저장하면, 같은 P의 후속 등록에서 ancestor/edge 작업을
생략해도 합집합은 같다. 단, H에서의 종류 suffix root는 후보마다 다를 수
있으므로 **hit에서도 현재 후보의 root를 반드시 추가**한다. 그 결과 물리
node·edge·H suffix root set 모두 기존 q15와 정확히 같다. 조기 reveal의
물리 경로, 종류 언어, 모든 원래 공급 ID의 정책 값과 semantic tie도 유지된다.

**Owner·실패·작업량 계약.** key는 owner와 plan에 묶이고 exact 비교한다. hash
충돌이나 cache 교체는 miss로 처리하며 다른 P의 certificate를 사용하지 않는다.
모든 predecessor 등록을 마친 뒤에만 publish한다. 실패·취소 중의 부분 cone은
완료 certificate가 아니며 owner의 기존 실패 처리를 따른다. 같은 후보의 여러
miss cone이 겹치는 경우에는 실제로 계산한 prefix를 한 번만 재사용한다.
H 조회, 새 ancestor 방문, 실제 typed edge 등록 시도와 cache publish를 누적
work에 청구하며 이미 수행한 일을 빼거나 한도를 재정의하지 않는다. cache와
scratch의 실제 capacity, 재할당 공존 peak를 owner 메모리에 포함한다.

`CLEARRA_OBSERVATION_PREFIX_CONES=0|1`은 기본 off인 별도 비교 축이다.
구현·독립 검토·141개 관측 회귀 및 실제 8/10/11-lock 계측을 완료했다.
8-lock ABBA는 coverage를 보존했지만 평균 4.0088527585→4.0511352495초로
시간 개선을 관측하지 못했다. 10-lock은 생산 work를 줄였지만 정책에서 미완료다.
node/edge/root set이 그대로라는 정리와 일치하게 11-lock coordinator node
상한도 남는다. 완료된 아홉 worker의 work 합계 71,396,217과 별개로 추가
중앙 병합 비용이 필요하므로, 이 결과를 전체 완주로 표시하지 않는다.

#### B19의 후속 q21: 완료된 중간 prefix cone과 exact-key 보존

B19의 독립성은 H에만 필요한 성질이 아니다. 활성 H 집합 P의 선행 집합
$T\subseteq P$가 한 합법 경로에 있고 $|T|\le H\le5$이면, T의 종류도 모두
distinct다. T까지의 물리 predecessor cone은 같은 요청의 초기 보드와 T의
원래 `(kind,cells)`만으로 정해진다. T 이후의 witness는 고정한 채 T까지의
다른 합법 순서를 붙여도 첫 여섯 distinct 공급 조건은 유지된다. 따라서 같은
T를 다른 후보나 다른 H cone에서 다시 만났을 때 완료된 T cone을 재사용할 수 있다.

재사용으로 생략하는 것은 **T의 선행 경로를 다시 확장하는 일**이다. 현재 child로
가는 T의 새 outgoing edge는 여전히 삽입해야 한다. 이미 방문한 parent라는 이유로
그 edge를 건너뛰면 서로 다른 H branch가 소실되므로 허용하지 않는다. H 자체의
hit에서도 현재 후보의 suffix root는 별도로 삽입한다. 새 certificate의 publish는
해당 선행 cone이 전부 완성된 뒤에만 허용한다.

같은 T는 원래 좌표의 삭제 행 집합도 결정한다. 구현은 그 값을 새 cache payload에
복사하지 않고 현재 geometry의 기존 `symbolic_deleted_rows(prefix)`를 재사용한다.
현재 후보의 다른 operation을 변환하는 frame도 그 후보의 geometry에서 만든다.
과거 후보의 전체 frame이나 미래 배치를 재사용하지 않는다. Exact key 비교와
owner 고정, worker-local ID 분리는 B19와 같다.

q18의 작은 direct map은 11-lock에서 4,563,703 miss 중 3,976,413회 교체를
기록했다. 후속 mode 2는 bounded exact-key 저장으로 이 재계산을 줄이는 후보이며,
원래 mode 1도 비교 대조로 남긴다. Cache 공간이 다 차면 기존 정확한 계산으로
돌아간다. 캐시의 상한은 결과 graph의 node 한도를 올리는 권한이 아니다. 보관·성장·
재배치·조회·publish의 실제 work와 공존 메모리를 포함하고 64M 누적 한도는 유지한다.
이 절은 일반 T의 전칭 재사용 근거이며 실제 완주·시간 개선은 별도 실행으로 판정한다.

q22의 mode 3은 같은 certificate의 보관 전략을 나눈다. H 자체는 기존 작은
direct cache에 두고, 엄격한 중간 집합 $T\subsetneq P$에만 exact map을 사용한다.
Mode 1/2/3이 인증하는 완료된 선행 cone의 의미는 같다. Cache 충돌·공간 부족은
다시 계산하는 비용만 늘리고, 아직 완성되지 않은 cone을 재사용하는 권한을 만들지
않는다. 이 구분은 q21 실제 P7P4에서 큰 H 집합을 exact map에 넣는 성장 비용이
생략한 경로 작업보다 컸다는 측정에 따른 것이다. Mode 3도 기존 다섯 H 계측과
아홉 exact 계측을 유지하며 두 계측의 work를 총 work에 중복 합산하지 않는다.

### B20. q19: 완료된 H 보드의 정확한 잔여 언어 class

전체 내부 parallel family가 완료됐고 더 들어올 shard가 없다는 입력 묶음에만
적용한다. B16 source scope, boundary suffix only, $1\le H<n$, edge closure 2,
동일 Plan과 destination kind arena를 추가로 요구한다. serial·외부·부분 family와
서로 다른 arena를 이 설계로 합치지 않는다. 최종 family 완료 권한은 batch merge가
성공한 뒤 기존 위치에서 발급한다.

실제 H 보드 b마다 **모든 worker**의 suffix root를 같은 kind arena에 rebind한 뒤
중복 제거한 완성 집합을 $R_b$라 하자. 집합의 저장 순서는 의미에 포함하지 않는다.
새로운 동치 관계는

\[
b\sim b'\iff (n-H,R_b)=(n-H,R_{b'})
\]

다. 이 조건이면 $\bigcup_{r\in R_b}L(r)=\bigcup_{r\in R_{b'}}L(r)$이다. H에서
원래 공급 reveal 권한을 얻은 소비자는 바로 이 종류 잔여 언어와 기존 Hold/cursor를
사용하므로, class로 치환한 후의 성공 여부도 모든 허용 공급 상태에서 같다.
서로 다른 root 집합이 같은 언어를 뜻하는 경우까지 판정할 필요는 없다.

pre-H의 실제 보드와 `(kind,cells)` 행동 label은 그대로 남긴다. 마지막 물리
간선의 place/clear·depth 검증은 **원래 child 보드**를 사용해 먼저 마치고 그 뒤
target ID만 class로 바꾼다. 여러 행동이 같은 class에 닿아도 행동을 제거하지
않는다. Physical state와 boundary class는 typed payload로 구분하며, 임의 대표
보드를 class의 실제 점유라고 간주해 기존 clear 검증에 넣지 않는다. H<n인 첫
범위에서는 class 자체가 accepting일 수도 없다.

완료된 worker의 H 입력을 `BoardKey`별 hash group으로 한 번 모으고 **전체 입력을
다 읽은 뒤에만** 각 보드의 class를 결정한다. Root는 exact key와 group generation
stamp로 중복을 먼저 제거한다. 순서와 무관한 fingerprint로 class 후보를 찾고,
remaining과 원소 수가 같으며 저장된 모든 root가 현재 group에 있음을 확인한다.
두 집합이 unique이므로 $C\subseteq R_b$ 및 $|C|=|R_b|$가 정확한 equality를
증명한다. Fingerprint 충돌은 이 전체 membership 검사로 해소한다. q19는 새 class일
때만 고유 root를 정렬했고 q22는 완료된 boundary에 한정한
`FrozenUniqueUnordered`를 사용해 그 정렬도 제거한다. 일반 mutable `Small`의
정렬·binary-search·삽입 계약은 바꾸지 않는다. 새 variant는 추가 수정이 금지되고,
소비자는 정확한 union membership 또는 자체 canonical member 정렬을 사용한다.
초기 설계의 worker별 H 비교 정렬·k-way 순회는
같은 증명을 따르지만 O(Σm log m)의 새 work가 커서 이 선형 grouping으로 바꾼다.
모든 실제 H 보드를 먼저 전역 graph node로 할당할 필요가 없다. 그렇더라도 원래
H 입력 수·고유 실제 보드 수·class 수와 staging 메모리는 별도로 계측한다.
보유한 pre-H node와 class는 기존 node 한도를,
보유한 edge와 class roots는 기존 edge/member 한도를 적용받는다. 원래 worker
work는 전부 한 번씩 더하고 새 집계·root 조회·remap·검증의 실제 비용도 청구한다.

class가 만들어진 뒤 새 후보나 shard를 받으면 완성 집합 전제가 깨지므로
producer-closed 상태를 별도로 둬야 한다. pre-H closure와 최종 freeze는 계속
허용하되 일반 등록·병합은 거절한다. 실패한 batch에서 일부 class만 성공 결과로
내보내지 않는다. 모든 살아 있는 worker physical/kind owner, destination arena,
typed rebound map, H grouping 인덱스와 remap, 한 보드의 root scratch 및 class index의
공존 메모리를 포함한다. 이 절은 설계·증명이며 구현과 계측 완료 상태는 §9에서
따로 관리한다.

q22는 원래 source node를 읽는 그 순회에서 typed remap을 한 번만 push한다.
각 원소는 실제 pre-H node 또는 아직 class가 결정되지 않은 H group이다. 모든
H group의 완성 root 집합을 확정한 뒤 `group.class`를 통해 target을 해석하므로,
source remap을 미리 초기화하거나 모든 H entry를 다시 쓰는 순회가 필요 없다.
공개 node ID와 group ID는 enum으로 구분하며 숫자 tag의 우연한 일치를 사용하지
않는다. 최종 간선 순회는 원래 outgoing edge가 있는 pre-H parent 목록만 읽는다.
원래 child 보드 검증을 remap 뒤로 옮기지 않으며, H 이후 간선을 새로 만들지도 않는다.
이는 같은 완성 함수의 계산 순서 변경이므로 B20의 양방향 경로 대응은 그대로다.
삭제한 실제 순회만 work에서 사라지며, 새 parent 목록·8-byte remap 및 공존 buffer
공간은 모두 계상한다.

## 5. 후보 C: 잔여 언어의 포함관계로 중복 제거

같은 공통 physical history 뒤의 residual 목록에 s,t가 있고, 현재 단계에서 보존해야
할 hybrid continuation 언어가 `L(s)⊆L(t)`라고 증명됐다고 하자. 숨은 구간의 label은
`(kind,cells)`, 인증된 경계 후 label은 종류다. 그러면

\[
L(s)\cup L(t)=L(t)
\]

이므로 이 **같은 union 내부에서** s를 제거해도 전체 언어가 정확히 같다. 다른
history의 후보들을 섞거나 서로 다른 첫 physical lock을 종류만으로 합치는 주장이 아니다.
언어 equality가 성립하므로 기존 residual derivative와 관측 귀납을 그대로 적용할 수 있다.

첫 구현은 정확한 root equality에 이미 쓰는 dedup보다 조금 강한 충분조건만 쓰면 된다.
같은 remaining depth와 accepting 조건에서, s의 모든 edge에 대해 같은 label을 가진
t의 어떤 edge가 대응하고 child도 같은 관계라면 directed simulation으로 inclusion을
증명할 수 있다. 깊이가 감소하므로 pair memo를 사용한 유한 귀납이다.

종류 DAG는 같은 종류의 child를 여러 개 가질 수 있다. 따라서 이 simulation은
language inclusion의 **충분조건**이며 항상 필요조건은 아니다. 하나의 s child를
여러 t child의 합집합이 함께 덮는 경우를 놓쳐도 원래 root를 남기면 정확성은 같다.
완전한 NFA inclusion을 무조건 계산하거나 모든 suffix word를 열거할 필요가 없다.

root가 m개일 때 전 pair 비교는 `O(m²)`개 query가 될 수 있고 각 query도 후속 DAG를
읽는다. equality, remaining depth, first-piece mask, 작은 exact edge-list 포함관계로
싼 경우부터 처리하고 전체 pair/edge 예산을 둔다. 예산 소진은 `NotProven`으로
처리해 원래 원소를 유지한다. 취소나 잘못된 ID는 `NotProven`과 별개 오류다.

현재 [`symbolic_prefix_subsumption.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_prefix_subsumption.rs)의
upper-kind-language가 proven kind alternatives에 포함된다는 증명은 oracle Boolean
언어용이다. 숨은 단계의 다른 footprint까지 포함관계를 제공하지 않는다.
`physical_action_complete`가 그 결과를 physical owner에 넘기지 못하게 하는 현재
권한 경계를 이 최적화를 이유로 제거하지 않는다.

다른 physical 행동 a,b 사이의 dominance는 별도다. 동일한 공급 갱신·관측 분할에서
a의 전체 정책을 b로 simulate할 수 있으면 최적 값은 보존되지만 원래 행동 언어와
동률 representative는 바뀔 수 있다. 이번 작은 antichain은 우선 같은 union 내부의
언어 equality에 제한한다. CountAll, spin/finesse·입력 이력의 multiplicity는 어느 쪽
증명에도 자동 포함되지 않는다.

### 추가 미적용 후보: 남은 7미노의 multiset별 word bitmap

Q4의 10-lock 실패는 등록 suffix roots가 8,000,000개에 도달한 경우였다. 이후 모든
kind root를 retained reference로 남기는 비용을 줄일 후보로, 같은 board와 같은
remaining multiset M에 속한 종류 word 언어를 정확한 bitmap으로 합칠 수 있다.
이 절은 **설계·증명과 손익 검토뿐이며 실제 구현은 아니다.** Q5의 더 싼 prefix
필터가 먼저 적용되는 것과 구분한다.

`r=Σ_p m_p≤7`인 multiset M의 서로 다른 word 수는

\[
P(M)=\frac{r!}{\prod_p m_p!}\le7!=5040.
\]

표준 kind 순서로 word를 lexicographic rank한다. 첫 종류가 p인 block의 크기는
`P(M−e_p)`이며 시작 offset은

\[
o_p(M)=\sum_{q<p,\,m_q>0}P(M-e_q).
\]

따라서 uniform-multiset canonical node v의 언어 bitmap은

\[
b_v=\bigvee_{(p,c)\in E(v)}\bigl(b_c\ll o_p(M_v)\bigr)
\tag{B}
\]

로 구성된다. Terminal의 M=0 bitmap은 빈 word 한 bit다. 같은 kind의 여러 child는
같은 block에 OR하고, 다른 kind의 block은 서로 겹치지 않는다. 각 child의 rank j가
parent의 `o_p+j`로 가므로 한 word/path씩 열거할 필요가 없다. DAG의 작은 remaining
depth부터 memo하면 식 B가 모든 word에 대해 원래 언어와 정확히 같다. 같은
`(board,M)`의 여러 root bitmap을 OR하는 것도 union의 정확한 표현이다. 물리 선택을
생략하는 권한은 여전히 B5/B6의 reveal certificate에 한정한다.

**Uniform multiset은 일반 cache의 현행 불변식이 아니다.**
[`piece_order_language.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/piece_order_language.rs)의
12-byte `CanonicalPieceNode`는 edge range, remaining depth와 accepting을 보유한다.
`remaining_depth`는 child 깊이 일치만 검사하며 종류별 multiplicity equality를
검사하지 않는다. 일반 same-depth NFA에 식 B를 무검증으로 적용하면 안 된다.

반면 typed `compile_product_closed_prefix_language_with_residuals`에서는 각 전이가
`S→S∪{i}`, terminal은 모든 operation의 집합이다. 따라서 suffix의 multiset은 정확히
`pieces[not S]`다. 동일한 살아 있는 word 언어를 canonicalize한 결과가 서로 다른
uniform multiset을 갖는 일도 없다. 구현한다면 이 typed provenance를 넘기거나,
node별로 terminal M=0과 모든 `M(child)+e_p`의 equality를 귀납 인증해야 한다.
Mixed multiset이나 r>7은 기존 표현을 사용한다. 현재 주요 8/10/11-lock fixture는
각각 L=1/3/4여서 board에 보관하는 suffix의 `r≤n−L=7`이 성립한다. 이것은 별도
ordinal codec의 범위를 넓히는 변경이 아니다.

Word payload만 본 최대 공간과 단순 4-byte root reference의 손익은 다음과 같다.

| 남은 r | 한 multiset의 최대 bit 수 | u64 bitmap payload | 같은 `(board,M)`에서 이 payload와 같은 크기의 root 수 |
| --- | ---: | ---: | ---: |
| 7 | 5,040 | 632 bytes | 158 |
| 6 | 720 | 96 bytes | 24 |
| 5 | 120 | 16 bytes | 4 |
| 1–4 | 최대 24 | 8 bytes | 2 |

실제 multiplicity가 반복되면 P(M)는 더 작다. 표는 bitmap metadata, group index,
hash/allocator 및 기존 root set capacity를 제외하므로 채택 기준의 충분조건은 아니다.
단일 root의 매우 작은 DAG 언어를 무조건 dense bitmap으로 바꾸면 공간이 늘 수 있다.
필요한 M만 만들고, 작은 root list·inline bitmap·가득 찬 permutation universe의
정확한 sentinel을 실제 분포에 따라 선택하는 후속 설계가 가능하다.

계산비용도 별도로 제한해야 한다. Root→bitmap cache는 한 canonical node를 한 번만
해석하지만, parent 구성에는 child bitmap word의 shift/OR, `(board,M)` 등록에는
다시 group bitmap OR가 든다. 간단한 상계는 `Σ_edges O(ceil(P(child)/64))`에 등록·merge의
bitmap word 작업을 더한 것이다. 8,000,000개의 bindings를 무조건 최대 79-word로
합치면 수억 번의 word 작업이 될 수 있어 기존 64,000,000 work 상한 안의 완료를
보장하지 않는다. Node당 r≤7이라는 사실만으로 전체 board 수나 distinct M 수가
작아지는 것도 아니다.

실제 손익을 판단하려면 `(board,M)`별 root 수와 P(M), canonical root 재사용 횟수,
bitmap 밀도·포화 여부, root 변환 word 수 및 group OR 횟수가 필요하다. 모든 retained
suffix가 bitmap으로 독립 표현될 때에는 worker의 kind-arena root rebind/import까지
생략할 가능성이 있지만, 이는 원래 ID cache의 lifetime과 fallback 소유권을 함께
바꿔야 하는 별도 구현이다. 현재 8M member cap을 단순히 키우거나 word 작업을
과금하지 않는 방식으로 완료를 주장하지 않는다.

### B21. q20: 실제 공개된 공급의 정확한 word bitmap membership

위 식 B의 첫 적용은 producer의 root binding을 대체하는 것이 아니라, 실제 reveal
뒤의 반복 membership을 줄이는 방향으로 한정한다. typed B16 board family의
H 경계, $1\le r\le6$, positive uniform source, 고정 horizon, terminal supply target
없음, 두 lookahead projection=false를 요구한다. 원래 observation node에서 관련
끝 위치 $\min(c+r+E,N)$까지 **실제로 읽을 수 있을 때만** query를 만든다.
`future_is_determined`만으로 숨은 source 배열을 읽는 권한은 발급하지 않는다.

query는 관측된 queue slice·길이·held kind·Hold 허용 여부와 remaining r을 exact
key로 보유한다. 기존 `SupplyExecutionAutomaton::sequence().transition`의
Current/SwapHeld/StoreCurrent를 사용해 r개의 lock word를 만든다. projected
held-only release는 이 범위에 없다. 같은 query와 multiset M에서 가능한 word의
rank bitmap을 $q_M$라 하면 정확한 성공 조건은

\[
\exists v\in R_b:\quad b_v\mathbin{\&}q_{M_v}\ne0.
\]

증명은 두 포함관계다. 교집합의 word는 kind DAG가 허용하는 물리 suffix와 기존
공급 automaton이 허용하는 Hold 실행을 모두 가지므로 성공이다. 반대로 기존
완성 경로의 word는 어떤 root 언어와 같은 automaton의 r-lock word 집합에 모두
속하므로 교집합에서 제거될 수 없다. 하나의 검증된 root에서 교집합을 찾으면
다른 unknown root가 있어도 성공을 확정할 수 있다. **실패는 모든 root에 대해
인증된 uniform multiset이 공급 word의 모든 multiset과 다르거나, 정확한 bitmap
교집합이 비었음을 증명한 경우에만** 확정한다. 전자는 bitmap 생성이 불필요하다. mixed multiset,
범위 밖 r 또는 선택적 cache 공간 부족은 `None`으로 기존 DP에 돌아간다.

관련 공급이 모두 공개된 현재 정보집합의 모든 원본 ID는 동일 query를 가진다.
따라서 membership은 그 집합 전부 성공 또는 전부 실패다. 기존 exact memo key로
그 값을 저장할 수 있고, 양성일 때 원래 subtree의 ID를 직접 수집할 수 있다.
새 언어 인덱스나 임의 큐 ordinal로 결과를 대체하지 않는다. pre-H의 물리 행동
순위와 분기별 값은 그대로이며, 이 Boolean 경로는 concrete suffix trace의 동일성을
새로 보증하지 않는다.

node bitmap은 같은 pinned kind arena에만 속한다. 깊이가 줄어드는 모든 child와
동일 종류의 모든 대안을 식 B에 OR하며, topology·remaining·acceptance가 어긋난
입력은 오류다. hash는 exact key 비교를 대신하지 않는다. query 생성, root 해석,
shift/OR/intersection의 실제 work는 기존 64M에 포함한다. 보유 u64 bitmap word는
보수적으로 u32 member 두 칸으로 산정하여 기존 base/kind/group와 합쳐 8M 안에
둔다. metadata·buffer capacity·공존 peak도 소유 메모리에 포함한다. 새 계측의
bitmap work는 kind work의 부분집합이므로 둘을 더해 중복 계산하지 않는다.

q20/q21의 구현은 유한 회귀를 통과했지만 실제 8-lock에서 64M work 한도로
중단됐다. r≤6의 720-bit 상한만으로 전체 입력의 작업량, 메모리, 10/11-lock 완료
또는 기존 DP보다 빠른 실행을 주장하지 않는다. 다음 q22는 같은 정확한 교집합의
반복 계산을 줄인다.

#### B21a. Target multiset에 대한 공급 단어의 재명명

관련 queue와 사용할 수 있는 Hold의 multiset을 A라 하자. 기존 query의 길이 계약은
$|A|\le r+1$이며 $|M|=r$이다. $M\nsubseteq A$이면 이 target의 공급 word는 없다.
그 밖에는 M에 등장하는 실제 종류를 작은 실제 kind 순서대로 1부터 재명명하고,
M 밖의 종류를 7로 보낸다. $|A|-|M|\le1$이므로 M 밖에 존재하는 조각은 최대 한
개다. 따라서 실제로 쓰인 alphabet에서는 이 대응이 단사이며 서로 다른 외부
조각을 하나로 합치는 문제가 없다.

기존 Hold automaton은 종류의 이름을 비교·저장·반환할 뿐 종류별 별도 규칙을
쓰지 않는다. 그러므로 모든 Current/SwapHeld/StoreCurrent 전이는 재명명과
가환한다. M 내부의 상대 kind 순서를 유지하므로 M의 서로 다른 permutation의
lexicographic rank도 재명명 전후 같다. **Canonical M counts, 전체 정규화 query,
r과 Hold 상태/허용 여부**가 같은 query bitmap은 정확히 같은 rank 집합이다.
물리 언어의 종류 대칭성을 가정하는 증명이 아니다. 물리 root bitmap은 실제
종류 그대로 유지하고, 공급 bitmap의 rank 해석만 이 동형으로 공유한다.

Target M의 남은 count가 0인 output branch는 어떤 M word로도 완성될 수 없으므로
즉시 가지치기한다. Rank는 이미 고른 prefix의 block offset을 누적해 계산한다.
이는 leaf마다 같은 word를 다시 rank하는 식과 동일한 합이며, 일반 automaton의
행동 선택이나 EOF 의미를 바꾸지 않는다. Raw query profile의 bounded cache도
동일한 exact QueryKey에서 A와 가능한 target 목록을 한 번 구하는 계산만 공유한다.
공간 부족은 기존 exact 계산으로 돌아가며 새로운 실패 언어로 처리하지 않는다.

#### B21b. Frozen board의 multiset별 OR와 완전 언어 certificate

같은 frozen board owner b와 target M에 대해

\[
B_{b,M}=\bigvee_{v\in R_b,\ M_v=M}b_v
\]

를 한 번 만든다. 결합·교환·멱등성에 의해
$B_{b,M}\mathbin{\&}q_M\ne0$은 원래 root별 교집합 중 하나가 양성인 것과
동치다. 아직 uniform multiset을 인증하지 못한 root의 존재는 별도 unknown bit로
남긴다. 검증된 양성은 unknown과 무관하게 유효하지만, 모든 교집합이 비어도
unknown이 있으면 기존 DP로 돌아간다. Board ID가 같아도 owner나 root 집합이
바뀌면 이 cache를 공유할 수 없다. 호출자는 immutable frozen view이며, cache miss의
root 검사와 pinned kind arena 권한을 유지한다.

OR가 모든 유효 rank bit를 포함하면 `Full`, 0이면 `Empty`, 나머지는 `Partial`로
저장할 수 있다. Full/Empty 판단은 실제 bit 전체를 검사해 인증한다. 범위 밖 bit나
잘못된 root를 임의로 Full로 간주하지 않는다. Full/Empty payload는 별도 bit 배열을
보유하지 않아도 되지만 metadata와 분류 scan의 비용은 여전히 보유량·work에 포함한다.

유효 query에서 **queue 길이 $\ell\ge r$이고 $M\subseteq A$이면 M의 어떤 r-lock
word가 기존 공급 automaton에서 실행 가능**하다. Hold 금지는 $\ell\le r$와 빈
Hold를 요구하므로 이 경우 $\ell=r$, $A=M$이고 Current만 사용하면 된다.
Hold가 허용되며 $|A|=r$인 경우도 Current로 전부 사용한다. $|A|=r+1$이면 남길
한 조각을 정한다. 처음 held 조각을 남기면 Current만 사용한다. Queue의 조각을
남기면 그 위치까지 Current를 사용하고, 빈 Hold이면 StoreCurrent, 찬 Hold이면
SwapHeld로 그 조각을 Hold에 남긴 뒤 나머지를 Current로 사용한다. Queue의
마지막 조각을 남기는 경우에는 그 앞 r개만 실행하면 된다. 따라서 구성 M을
공급하는 적어도 한 합법 word가 존재한다. 반대로 구성 부족은 불가능하며,
$\ell<r$이면 각 lock이 적어도 한 current draw를 소비하므로 r lock은 불가능하다.
이 정리는 projected EOF held release를 허용하지 않는 현재 query 범위에 한정한다.

그러므로 Full과 위 공급 조건을 함께 인증한 경우 query bitmap을 만들지 않고
양성을 반환해도 정확하다. Empty는 해당 target에 기여하지 않으며, 모든 target이
음성이고 unknown도 없을 때만 전체 음성을 반환한다. 일반 Partial은 정확한
target query bitmap과 교집합을 계산한다. 이 세 표현은 정책의 원래 ID 수집,
가중치, 사전 공개 권한 및 tie 계약을 바꾸지 않는다.

Root/query/board의 실제 bitmap payload는 모두 기존 8M member credit을 공유하고
u64당 두 칸으로 센다. Query profile·canonical target key·board metadata, 선택적
eviction, capacity와 공존 peak도 계상한다. 새 board 계측의 work는 전체 kind work에
이미 포함된 부분량이며 총 한도를 늘리거나 같은 작업을 두 번 합산하지 않는다.

### B22. q23: 요청 단위 공유 등록과 완료 후 H 확정

q23은 native parallel의 같은 요청에서 geometry·source-qualified 관계와 H suffix
종류 언어를 계속 각 worker가 계산하되, 완성된 기여의 physical 등록을 하나의
request owner에 직렬화한다. Local A/B mode 1, B16의 원래 공급 scope,
source filter/existence·board·boundary suffix·완료 boundary class,
edge closure 2와 reachable suffix import의 권한을 요구한다. 자동 worker 수나
source family를 줄이는 최적화가 아니다. Movement search는 등록 잠금 밖에 있다.

후보 c의 source-qualified 선행 기여를 $E_c$, 실제 H 보드 b의 정확한 suffix
언어 root 기여를 $R_c(b)$라 하자. B19의 완료 cone 재사용은 각 기여를 계산하는
방법만 바꾸므로, 완전 입력 집합 C에서 필요한 결과는

\[
E=\bigcup_{c\in C}E_c,\qquad R(b)=\bigcup_{c\in C}R_c(b)
\]

다. Worker별로 먼저 합집합을 만든 다음 중앙에서 다시 합쳐도, 같은 owner에
각 완성 기여를 바로 합쳐도 결합·교환·멱등성으로 E와 R은 같다. Numeric ID와
처리 순서는 이 집합의 의미에 포함하지 않는다. 동일 physical label의 결과 보드는
결정적이며, 모든 실제 outgoing edge와 각 후보의 현재 H suffix는 계속 삽입한다.
완료된 cone의 hit를 이유로 새 outgoing edge나 새 suffix 기여를 생략하지 않는다.

생산 중에는 pre-H physical node와 **pending H group**을 구별한다. Pending group은
실제 BoardKey와 mutable unique roots를 보유하며 실행 가능한 정책 node가 아니다.
서로 다른 H 보드를 현재 root 집합이 같다는 이유로 먼저 합치지 않는다.
마지막 physical edge는 원래 parent/group 보드의 place-and-clear 및 depth를 검증한
후 group을 참조한다. 모든 canonical branch와 worker 결과가 완료되고 취소·부분
생산이 없음을 확인한 뒤에만 B20의 exact root-set class를 만든다. 이어 고정된
group→class 대응으로 incoming edge를 해석하고 기존 pre-H closure/freeze를 수행한다.

그때 얻는 graph와 kind 언어는 기존 완료 batch와 같으므로 B5의 complete-family
보드 몫과 B20의 실제 reveal 후 정책 귀납이 그대로 적용된다. 공개 전 physical
행동의 의미 순위도 유지한다. 원래 Ω와 원본 ID의 정책 결과 보존은 이 같은 tie
계약에서의 주장이다. 특정 suffix witness의 방문 순서나 임의 raw child ID 순서를
제품 계약으로 추가하지 않는다. 등록 중간의 부분 graph에 이 정리를 적용하거나,
그 부분 결과를 정확한 확률 0으로 내보낼 수는 없다.

#### 종류 언어의 증분 재바인딩

Worker의 source→destination map은 두 arena의 identity를 각각 Arc로 고정한다.
Source append는 기존 완료 map entry를 보존한다. 새 root가 필요하면 아직 매핑되지
않은 reachable child만 postorder로 가져온다. 모든 같은 종류 NFA 대안을 복사하고,
child mapping 완료 뒤 기존 exact edge-list interner로 parent를 만든다. 깊이에 대한
귀납으로 각 매핑 전후의 종류 언어가 같다. Hash의 일치만으로 동치를 부여하지 않는다.
이미 완료된 root는 map으로 직접 해석하므로 전체 source map 초기화나 두 번의 전체
node scan을 반복하지 않는다.

Source arena를 재활용할 때는 map과 source pin을 명시적으로 reset한다. Destination
pin과 누적 계측·peak·실패 상태는 유지한다. 다른 destination에 map을 적용하는 일과
실패한 importer를 reset으로 되살리는 일은 거절한다. 완료 기여를 등록하는 동안
source를 불변으로 빌리고, 반환 뒤 global roots는 destination의 owned node만 참조한다.
따라서 worker source의 해제나 다음 epoch가 이전 numeric ID의 의미를 바꾸지 않는다.

#### 한도·실패·소유 메모리

Pending H의 실제 root reference도 삽입할 때부터 기존 **8M member**에 포함한다.
Staging이라는 이름으로 제외하지 않는다. Pre-H와 확정 class는 **1M nodes**,
보유한 pre-H 및 pending incoming edge는 **8M edges**를 계속 적용받는다.
동등 class의 중복 root reference를 실제로 폐기할 때만 live member 수가 줄며,
이미 수행한 work는 차감하지 않는다. Finalization 전에는 그룹이 실행 node가 아니므로
중간의 H board 전체를 먼저 executable node로 할당할 필요가 없다.

새 잠금 단계, 등록/cache, 실제 group/root/class/edge 확정 작업은 기존 physical
**64M 누적 work**에 청구한다. Worker source compile과 kind import의 구조 계측은
기존 별도 domain을 유지하며 물리 work를 초기화하거나 대신하지 않는다. 증분 import의
13계측은 실제 방문·맵 확장·삽입 및 이동 상계를 보고한다. 이 수를 interner의 모든
CPU 산술·hash·sort 연산 수로 새롭게 해석하지 않는다. 전체 생성·정책 시간과
감독 RSS를 함께 측정하므로 작업 domain별 숫자만으로 성능을 판정하지 않는다.

공유 physical/kind/staging은 요청 소유량으로 한 번 센다. Worker의 private source
arena·scratch·증분 map과 그 성장 overlap은 각 worker에 남긴다. 확정된 공유 payload를
coordinator workspace로 이전한 뒤 공유 snapshot을 다시 더하지 않는다. 취소, peer
abort, poisoned lock, 잘못된 owner와 부분 family는 실패를 유지하고, 새 class의
정상 결과로 바꾸지 않는다. 최초 원인 진단과 실패 시 peak도 보존한다.

## 6. 후보 D: 관측 trie를 만들기 전에 공급 미래를 factor

현재 `ObservationTrie::compile`은 원래 모든 ID의 단어를 insert하고, 이후
`compile_future_classes`가 같은 weighted suffix tree를 합친다. 후자는 정책 memo
중복을 줄이지만 이미 만든 trie node와 terminal 원소의 비용을 없애지는 않는다.

완전 standard 7-bag의 source 미래는 `(관측된 draw 깊이 d, 그 시점의 bag remainder)`로
표현할 수 있다. 남은 bag이 비었을 때 새 bag을 여는 전이, 유효 source 길이와 EOF를
포함한다. 여기서 bag remainder는 cursor가 아니라 **현재까지 공개한 prefix** 뒤의
미래를 뜻한다. cursor와 visible window는 별도 정책 상태에 그대로 남는다.

### D1. 직접 공급 DAG의 동치

같은 source state에서 가능한 다음 조각, 갱신된 source state, terminal, 각 subtree의
원래 ID 개수·질량이 정확히 같다고 하자. 이 상태를 node 하나로 표현하고 원래
observation prefix에는 그 node와 원래 ordinal 구간의 대응을 붙인다.

**정리 D1.** 이 표현은 raw trie의 weighted future class를 선제적으로 구성한 것과
같다. terminal에서 같은 ID 개수·질량을 대응하고, 깊이 d에서 각 조각 child와
관측 block이 대응한다. 남은 draw 수의 귀납으로 원래 suffix 단어와 가중 leaf가
보존된다. 동일한 visible/hold/cursor/terminal 데이터를 남긴 정책 재귀는 관측 몫
정리 7에 의해 같은 값이다. □

동일 lexical prefix의 standard-bag words는 연속 ordinal 구간을 이루므로 자식의
순서와 suffix count로 그 구간을 분할할 수 있다. 완전 cover가 된 block은 구간을
원래 bitset에 넣고, 부분 성공이면 선택 정책의 reveal을 따라 다시 나눈다. 따라서
source DAG node를 공유하더라도 서로 다른 원래 prefix의 ID를 섞지 않는다.
이미 있는 standard-bag cursor 전이·suffix count·ordinal materialization 계산을
재사용하는 것이 별도 bag 구현을 만드는 것보다 적절하다.

완전 P7P3/P7P4의 plain prefix trie 크기는 다음 조합식으로 계산된다.

\[
\#\text{trie}(7+k)=\sum_{d=0}^{7}7P_d
   +7!\sum_{d=1}^{k}7P_d.
\]

| 원래 source | 원래 ID 수 | plain prefix trie node 수 |
| --- | ---: | ---: |
| P7P3, 10 draw | 1,058,400 | 1,319,060 |
| P7P4, 11 draw | 4,233,600 | 5,552,660 |

이 표는 완전한 중복 없는 standard-bag word 집합의 정확한 조합 수다. 실제 allocation
capacity나 실행 로그에서 읽은 수치가 아니다. 직접 source 전이는 `O((N+1)·128)`개의
깊이/remainder 슬롯 안에서 표현할 수 있지만, 전체 정책 상태 수·visible window
수·원래 성공 bitset의 비용까지 그 크기로 줄어든다는 뜻은 아니다.

처음 적용할 scope는 `Standard7BagLexicographic`, fresh source, 완전 ordinal domain,
확인된 공통 양수 가중치, 추가 terminal target 없음으로 제한할 수 있다. 잘린
lexicographic prefix를 지원하려면 각 구간을 materialized 끝점과 정확히 교차시킨다.
임의 sparse 원래 ID index, 서로 다른 가중치, 중복 단어는 일반 trie 경로를 유지하거나
별도의 정확한 ID/weight mapping을 제공해야 한다. projected last piece는 원래
materialized EOF를 늘린 원래 ID로 세지 말고 동일 ID의 결정적 전이로 표현한다.

## 7. Geometry/Kick `off/current/improved`의 정확한 비교

실제 대상은 `ReachabilityTemplate::compile_normalized`가 생성하는 reverse rotation
CSR이며, 원본 forward 후보 목록이나 유효 geometry skeleton 자체를 지우는 작업이
아니다. source·선택 target의 required-empty mask를 E, fixed occupied를 F,
variable cells를 V라 두면 E2는 다음과 같다.

\[
F\cap E=\varnothing\quad\land\quad
\forall\text{ 이전 valid 후보 }M:\
 (F\cap M\ne\varnothing)\lor((M\cap V)\setminus E\ne\varnothing).
\]

현재 전역 판단은 `F=∅, V=전체 field`다. 앞선 valid mask 하나라도 E 안에 완전히
포함되면 source/target이 free일 때 그 앞 후보도 항상 free여서 later edge는 절대
first success가 될 수 없다. 반대로 모든 앞 후보에 E 밖 셀이 있으면

\[
B^*=\text{field mask}\setminus E
\]

가 source/target을 비우면서 모든 앞 후보를 동시에 막는 하나의 보드 witness다.
따라서 이 판정은 **어떤 보드에서라도 그 edge가 활성일 수 있는가**의 필요충분조건이다.
모든 footprint가 field 안에 있어 코드의 `u64::MAX` variable mask와 이 witness의
판정은 동일하다. source의 sky 도달성이나 target의 grounded lock 여부까지 판정하는
것은 아니다.

같은 F,V와 같은 단일 edge 정보만 쓰는 다른 술어로 더 많은 edge를 안전하게 지울 수는
없다. 더 강한 제거에는 인증된 좁은 cube, target predecessor cone, 실제 경로 조건 등
추가 정보가 필요하다. 좁은 cube에서 만든 음성 증거를 더 넓은 family에 재사용하거나,
최소/최대 보드 두 개의 도달성으로 ordered kick의 중간 보드를 추론하지 않는다.

| 모드 | count pass | fill pass | 의미 |
| --- | --- | --- | --- |
| off | 모든 유효 원본 candidate를 센다. | 모두 원래 순서로 넣는다. | 독립 비교 기준. |
| current | 각 candidate의 E2를 평가한다. | 같은 E2를 다시 평가한다. | 현행 edge 집합. |
| improved | E2를 한 번 평가하고 keep bit를 저장한다. | 저장한 keep bit만 읽는다. | current와 정확히 같은 edge 집합 및 CSR 순서. |

**정리 G.** 입력 template가 두 pass 사이에 불변이고 candidate ordinal별 bit가
일대일이면, current의 두 번째 판단은 첫 번째와 같은 순수 술어다. improved가 그
결과를 읽으므로 count, offset, fill 위치, 원래 `prior_target_count`가 모두 같다.
따라서 reverse CSR는 element별로 같고, 이를 사용하는 모든 후속 경로도 같다. □

forward candidate 수를 E_total이라 하면 추가 일시 저장은
`ceil(E_total/64)·8` byte다. 그 저장은 template 빌드가 끝나면 사라지며 실제 peak
projection과 함께 비교해야 한다. expected 이득은 E2 및 prior-mask 재검사의 감소다.
steady-state search에서 pruning 강도가 더 세졌다고 표현하지 않는다.

삭제된 reverse edge의 target도 살아 있는 다른 later edge의 first-success를 막을 수
있다. 그러므로 모든 모드에서 forward 목록과 순서, raw ordinal, 앞 후보의 충돌 읽기를
보존한다. `no-kick`/O 전용 단조성은 실제 table capability로 별도 인증한다.

## 8. 평가와 제품 계약

증명과 실험의 역할을 섞지 않는다. 위 정리는 전제를 만족하는 임의 깊이·보드·공급
경로에 대한 논증이다. 회귀는 코드가 그 전제를 잘못 연결한 오류를 찾고, 계측은 실제
자원 이득을 확인한다. 짧은 성공 실행 몇 번으로 P7P4 complete를 대신하지 않는다.

| 축 | 같은 의미인지 확인할 대상 | 비용을 분리해 기록할 대상 |
| --- | --- | --- |
| Geometry off/current/improved | 같은 실제 lock relation. current/improved는 CSR와 원래 ordinal도 element별 equality | template compile 시간, E2 candidate/prior 검사 수, retained reverse entries, keep bitmap peak |
| 기존 Physical 경계/A1 Frontier | same information set에서 동일 optimum count/weight, 올바른 선택 ID, reveal 이전 행동 제한 | physical/revealed/frontier node와 member 수, source visits, owner와 view의 retained bytes, policy wall time |
| A3 live relation / A4 reveal 하계 | 같은 active labelled relation, 전체 원래 source 인증, 모든 실제 reveal query의 suffix root equality | 등록 candidate/relation 수, retained suffix binding 수, 등록·merge work, 별도 정책 평가 시간 |
| A5 original-universe index 생략 | 같은 전체 원래 ID·가중치·trie·projected 전 EOF, subset 경로 분리 | 생략한 position bitplane/identity index의 시간·메모리와 여전히 필요한 trie 생성 비용 |
| B의 board score memo | 각 원래 residual에서 계산한 값과 선택 정책의 원래 ID 성공. family certificate 유무 | raw residual 수와 memo class 수, memo hit, 피한 derivative/source visits, certificate 비용 |
| B5의 freeze된 board DAG | 동일 원래 Ω product의 root optimum, 실제 place/clear 전이와 PC terminal, 전체 생산 권한 | candidate/prefix 방문량, unique board/edge/suffix root 수, 등록·merge·freeze·정책 시간과 각각의 peak |
| B6의 q4 revealed kind union | 공개 뒤 종류 언어 equality. 같은 frozen physical producer의 mode 0/1에서는 같은 값과 원래 selected ID 집합 | union node/edge/member/source visit/work, 정책 상태 수와 시간, owner와 view의 별도 메모리 |
| B7의 q5 fresh-bag prefix 제한 | 전체 원래 Ω와 product한 root 정책 equality, 제한 미인증 시 기존 경로 | 직접 source-pruned prefix와 productive 후손 없는 prefix 수, 등록 node/edge/suffix/work, 전체 시간과 peak |
| B8의 q6 H-only suffix | 같은 원래 Ω의 root 값·selected IDs, 조기 reveal 뒤의 공급·hold·EOF와 H 전환 경계 | H 전 suffix binding·merge 감소, 추가 물리 정책 상태·검사, source/freeze/policy 단계와 peak |
| B9의 q7 legal-edge 및 q9 hybrid 재구성 실험 | 기존 edge 포함·새 edge의 실제 물리 soundness, 같은 Ω에서 positive 행동·selected IDs 보존. 같은 층 predicate를 등록과 finalizer가 공유 | producer의 생략한 edge 반복, full pose 대상·hard-drop/cache 양성·desired 탐색 방문 수, finalizer 시간·work·peak. q9 8-lock 세 mode 완료, P7P4는 freeze 뒤 정책 work 상한 |
| B10의 q8 full-count collection | 현재 정보집합 전체와 같은 count에서 원래 ID·가중 질량·고정 accepted depth 보존, variable horizon은 기존 경로 | q9의 같은 EDGE=2에서 collection 0/1 완료·원래 ID equality 확인. 수집 호출 감소와 여전히 남는 policy value 비용을 구분 |
| B11의 q10 observed-supply count filter | 원래 실제 관측으로 얻은 개수 상계, 모든 NFA edge의 uniform multiset 증거, 각 q와의 product equality. unknown은 보존 | 92개 focused 검증과 8-lock ABBA 완료. 전체 시간은 줄고 charged work는 늘었다. P7P4는 정책 work 상한으로 미완료 |
| B12의 q11 prefix word mask | exact operation 순서·H·cap에서 scalar와 동일한 prefix·productive graph·suffix. 부분 cache miss는 기존 경로 | 101개 focused 검증과 8-lock ABBA 완료. 8-lock 평균 시간 개선 미관측, 10-lock은 방문 감소에도 미완료. 기본 off |
| B13의 q12 suffix 부분 import | 보유한 모든 root의 reachable labelled DAG, 두 owner identity, 기존 destination ID | map 전체 scan과 실제 reachable 방문, source/destination/map/성장 peak. 5개 subset 검증 통과, 실제 계측은 별도 기록 |
| B14의 q13 변환 전 DP 재사용 | 같은 원래 supply memo state의 정확한 score/count, 기존 canonical memo와 ID replay | normalization이 실제로 생략된 hit, 65,536-entry fallback·retained/peak. 새 7개를 포함한 관측 110개 검증 통과 |
| B15의 q14 fresh-bag 존재성 | 원래 전체 Ω에서 oracle-positive candidate의 필요충분 판정, Hold/EOF와 projection 경계 | 생략한 oracle 작업과 새 witness·suffix 작업을 구분. 실제 8-lock 시간 개선 미관측, 10/11-lock 미완료 |
| B16의 q15 source-qualified 경로 | 모든 원래 q와의 성공 경로 product, full-family board closure의 root 정책 값·양수 행동·원래 ID | 추가 source mask/closure 생성과 생략한 q5/q11 검사, physical 등록·merge·정책 비용을 모두 포함해 A/B 측정 |
| B17의 q16 exact multiset group | 가능한 공급 multiset에 속하는 모든 root와 모든 unknown root 유지 | group 생성·조회·optional eviction work와 payload; 기본 off, 8-lock 총시간 개선 미관측 |
| B18의 q17 행동 상계 | 아직 계산하지 않은 원래 ID 수를 더한 상계와 기존 tie 순서로 exact incumbent을 이길 수 없는 행동만 생략 | bound 조회·pruned 행동·생략한 reveal branch; 8-lock ABBA 후 guard를 유지해 기본 on |
| B19의 q18/q21 predecessor cone | 같은 요청·원래 kind/cells의 전체 선행 cone, 새 outgoing edge와 매 후보 H suffix 보존 | H hit/miss, 중간 T 조회·재사용, cache 초기화·성장·재배치·실제 누적 work 및 retained/peak |
| B20의 q19 완료 boundary class | 모든 shard를 읽은 뒤 같은 remaining과 전체 root 집합; 원래 child clear 검증 뒤 remap | raw H와 고유 보드/class 수, retained roots, staging peak, 추가 work; 161개 관측 통합 검증에 첫 5개 회귀 포함 |
| B21의 q20 실제 공개 membership | 실제 알려진 Hold/큐 word와 uniform multiset DAG 언어의 정확한 교집합, 원래 ID 전부/없음 | query 생성/cache hit, bitmap nodes/words, intersection·fallback 및 기존 kind work에 포함된 추가 work |
| C의 antichain | bounded proof가 승인한 union의 언어 equality | inclusion query/edge 수, proof budget exhaustion, 제거 root 수, saved membership 비용 |
| C의 r≤7 multiset bitmap 후보 | uniform multiset의 exact word 언어와 OR/derivative equality | `(board,multiset)`별 root 중복, 실제 bitmap payload·밀도·word 연산 수, arena rebind 제거 가능 범위. 현재 미구현 |
| D의 source factor | raw trie와 동일 reveal block·ID·질량·EOF 및 선택 bitset | source construction 시간, retained source nodes, ordinal 복원 시간, policy state 수 |

CountUnique는 성공 여부와 큐 확률의 의미다. 언어 union이 중복 경로를 지우는 것이
CountAll execution multiplicity, score/finesse/입력 이력을 보존하지는 않는다.
원래 동률 action order를 바꾸면 optimum이 같아도 selected ID 집합이나 대표 실행이
달라질 수 있다. 그 출력을 동일하게 유지해야 하는 경로에서는 stable 원래 label/order를
유지해야 한다. value memo를 공유하고 원래 graph에서 선택을 재구성하더라도 lazy
node 할당 순서가 달라지면 raw numeric child ID를 사용하는 동률 순서가 바뀔 수 있다.
따라서 legacy tie까지 동일하다는 주장은 별도 ID 불변식이 필요하다. stable physical
action key를 새로 정하면 값과 가능한 성공 정책은 보존할 수 있지만 이전의 우연적
numeric-ID tie에서 선택한 원래 ID 집합까지 같다고 주장하지 않는다.

기존 kind-root와 새 class는 owner identity와 remaining depth로 namespace를 분리한다.
hash collision은 항상 전체 signature 비교로 확인한다. worker별 값/선택 bitset을
합쳐 전역 정책으로 만들지 않고, 모든 완전한 producer 관계를 합친 뒤 평가한다.
취소, storage refusal, work cap, 불완전 producer는 완료된 확률 0이 아니다.

## 9. 구현·검증 상태와 남은 작업

체크는 공동 작업에서 실제 구현·검증한 범위를 뜻하며 이 문서 작성자가 Cargo나
benchmark를 다시 실행했다는 뜻은 아니다. 상세 측정은
[별도 실행 기록](see7-pruning-measurements-2026-11-22.md)에 둔다. 이하 artifact는
모두 `_local/artifacts/see7-quotient-pruning-20261122` 아래 파일을 가리킨다.

q3 최초 전체 debug 검증은 **1,088 passed, 1 fixture failed, 14 ignored**였고,
해당 fixture 수정 및 불완전 probability 출력 회귀 추가 후 observation focused
검증은 **68 passed, 0 failed**였다. 근거는 `quotient3-debug-tests.log`와
`quotient3-observation-tests.log`다. q4 observation focused 검증은
`quotient4-observation-tests.log`의 **74 passed, 0 failed**, q5는
`quotient5-observation-tests-fixed.log`의 **78 passed, 0 failed**다. q5 release
빌드 완료도 `quotient5-release-build.log`에 기록되어 있다.

q6는 EOF held-release를 의도한 fixture에 해당 권한을 정확히 부여한 뒤,
`quotient6-observation-tests-fixed.log`에서 **80 passed, 0 failed, 5.24 s**를
확인했다. compile/test 감독 receipt는 `1795326246286803716-5`다. q6 소스는
`eb55e5cf290cc30f063bdc89ec2d079cd3ce8eb4`이며 release 빌드 완료는
`quotient6-release-build.log`와 receipt `1795326288788761890-5`에 기록되어 있다.
실제 8-lock boundary mode 비교는 완료했고 P7P4는 work 상한으로 미완료다.
이 q6 focused 결과 자체를 수정 후 전체 테스트 재실행 통과로 확대하지 않는다.

q7 소스 `f689e67421670dd487168b1a4657adfc71fca3ce`의 전체 검증은
`quotient7-full-core-tests-fixed.log`에서 **1,104 passed, 0 failed, 14 ignored,
167.61 s**이며 감독 receipt는 `1795327175040614420-5`다. 이후 board view의
profile 설정을 **EDGE=1에서만** 수행하고, 그때도 요청의
`conditioned_reachability_enabled()` 값을 보존하도록 guard를 수정했다.
수정 후 `quotient7-observation-tests-final.log`의 **81 passed, 0 failed, 5.45 s**와
receipt `1795327459787149276-5`를 확인했다. 후자의 focused 결과를 guard 수정 뒤의
전체 재실행으로 표시하지 않는다. 기본 EDGE mode는 0이며 release 빌드는
`quotient7-release-build.log`에서 **1m 41s**에 완료했다. 실제 P7P4 EDGE=1은
아래와 같이 finalizer의 work 상한으로 미완료다.

q8/q9 소스는 `dcf06bc726fdaf9fc1a0dfcf7d2ce65c5318e835`다. 첫 관측 batch의
`quotient9-observation-tests.log`는 85 passed / 1 failed이며 terminal-target fixture
생성의 `NextCycleRemainingPieceCountInvalid` 오류였다. 수정 후 기본 기능의
`quotient9-observation-tests-fixed.log`는 **84 passed / 0 failed**, parallel/local A/B의
`quotient9-observation-parallel-tests.log`는 **86 passed / 0 failed**다. 둘은 겹치는 feature
구성이므로 독립 테스트 170개로 합산하지 않는다. release build는
`quotient9-release-build.log`에서 완료했으며 실제 binary SHA는
`abeb8f5a230813009592174b9a7ad72fb7096682a79e69d4ef582a10de91ebce`다.
q7 전체 core suite를 q9 exact-head 전체 재실행으로 표시하지 않는다.

q10의 첫 focused 로그 `quotient10-observation-tests.log`는 **91 passed / 1 failed**이며,
관측 probe의 `prepare_trie` 누락으로 `wasm_observation_future_class_missing`가 발생했다.
세 probe의 준비를 연결한 뒤 `quotient10-observation-tests-fixed.log`는
**92 passed / 0 failed / 4.29 s**, receipt `1795329788399199036-6`를 기록한다.
해당 소스는 `d17c26e5f8d411f81c3fc7bd00f22959dbe772b7`이며 큰 입력 실측 및
전체 suite 완료를 뜻하지 않는다. q10 release binary SHA는
`fe27586f2adf2679c045665f328cf24cc94017b4f1c0f21c015aa821428d708d`이며,
실제 P7P4는 아래의 공통 work 상한으로 실패했다. 8-lock의 같은 binary ABBA는
완료했으며 평균 36.9037816145→10.6043791735초와 원래 coverage 일치를 기록했다.

q11은 새 prefix-mask helper와 board register/merge/메모리 계상, policy/result의 여섯
계측 및 benchmark v9 mode 연결을 완료했다. 새 helper 회귀와 on/off·merge 검증을
포함한 관측 101개가 통과했고 release 및 실제 8-lock ABBA도 완료했다. 평균 시간
개선은 관측하지 못했으며 P7P4는 작업 한도에서 미완료다. 기본 `PREFIX_MASK=0`을
유지한다. q10의 92개와 겹치는 테스트를 독립 검증 수로 합산하지 않는다.

| 실행 | 실제 경과 시간 | 완료와 주요 결과 | 근거 artifact |
| --- | ---: | --- | --- |
| q3 8-lock factored | 204.153660270 s | complete. 211,234 / 211,680 original IDs, policy states 161,008,165 | `quotient3-eight-factored.json` |
| q3 8-lock board | 231.933298904 s | complete. 같은 original IDs, policy states 189,236,250 | `quotient3-eight-board.json` |
| q4 8-lock board + kind alternatives | 233.053567468 s | complete. 같은 q4 binary의 kind-union mode 0 대조 | `quotient4-eight-alternatives.json` |
| q4 8-lock board + kind union | 43.714876175 s | complete. 같은 original IDs, policy states 35,087,187, action checks 56,339,679 | `quotient4-eight-union.json` |
| q5 8-lock source prefix 0 | 42.104770599 s | complete. q5의 source-pruning off 대조, 같은 original IDs | `quotient5-eight-source-off.json` |
| q5 8-lock source prefix 1 | 42.220748636 s | complete. comparator validation/equality 통과. 이 pair에서 총시간 개선은 관측하지 못함 | `quotient5-eight-source-on.json`, `quotient5-eight-comparison.json` |
| q6 8-lock boundary suffix 0 | 43.755515787 s | complete. 같은 q6 binary에서 board/kind/source mode 1을 고정한 대조 | `quotient6-eight-boundary-off.json` |
| q6 8-lock boundary suffix 1 | 40.142549954 s | complete. 같은 원래 coverage와 완료 계약, comparator validation/equality 통과 | `quotient6-eight-boundary-on.json`, `quotient6-eight-comparison.json` |
| q9 8-lock edge 0 / collect 0 | 43.442057414 s | complete. 같은 q9 binary 네 arm의 첫 대조 | `quotient9-eight-edge0-collect0.json` |
| q9 8-lock edge 1 / collect 0 | 40.934670986 s | complete. 같은 원래 ID·coverage, 전체 legal-edge closure | `quotient9-eight-edge1-collect0.json` |
| q9 8-lock edge 2 / collect 0 | 38.323224955 s | complete. 같은 원래 ID·coverage, hybrid edge | `quotient9-eight-edge2-collect0.json` |
| q9 8-lock edge 2 / collect 1 | 37.415919910 s | complete. 같은 원래 ID·coverage, full-count 수집 shortcut | `quotient9-eight-edge2-collect1.json`, `quotient9-eight-comparison.json` |
| q3 10-lock P7P4 board | 97.570835982 s | incomplete, `wasm_observation_physical_union_limit` | `quotient3-p7p4-board-probe.json` |
| q4 10-lock P7P4 board + kind union | 88.292204304 s | incomplete. 정책 평가 전 등록 중 suffix root 8,000,000 상한 도달 | `quotient4-p7p4-union.json`, `quotient4-p7p4-union.stderr` |
| q5 10-lock P7P4 source prefix 1 | 97.094348033 s | incomplete. incoming shard의 work 사전 청구가 64M 상한을 초과 | `quotient5-p7p4-union.json`, `quotient5-p7p4-union.stderr` |
| q6 10-lock P7P4 boundary suffix 1 | 92.704597752 s | incomplete. incoming shard의 work 사전 청구가 64M 상한을 초과 | `quotient6-p7p4-union.json`, `quotient6-p7p4-union.stderr` |
| q7 10-lock P7P4 edge closure 1 | 96.746755932 s | incomplete. 전체 456,459개 후보 생산 뒤 edge finalizer에서 64M work 상한 도달 | `quotient7-p7p4-edge-on.json`, `quotient7-p7p4-edge-on.stderr` |
| q9 10-lock P7P4 edge 2 / collect 1 | 107.122856232 s | incomplete. 전체 후보 등록·freeze 완료 뒤 정책 kind union이 공통 64M work 상한에 도달 | `quotient9-p7p4-edge-hybrid.json`, `quotient9-p7p4-edge-hybrid.stderr` |
| q10 10-lock P7P4 edge 2 / supply multiset 1 | 93.529510444 s | incomplete. 전체 후보 등록·freeze 뒤 root 414,839개 중 390,547개를 제외했지만 정책과 producer의 합이 64M work 상한에 도달 | `quotient10-p7p4-multiset-on.json`, `quotient10-p7p4-multiset-on.stderr` |
| q5 11-lock / 12-draw 제한 probe | 180 s 감독 제한 | timeout. JSON이 비어 있어 완료 확률·coverage·내부 종료 단계는 확인할 수 없음 | `quotient5-eleven-probe.stderr` |
| q9 11-lock / 12-draw | receipt 419,282 ms | SIGKILL, exit 137. JSON 0 bytes, 내부 단계·원인 불명. 600 s timeout으로 표시하지 않음 | `quotient9-eleven-edge-hybrid.stderr`, receipt `1795329207758780571-5` |

표의 모든 8-lock 완료 실행의 원래 coverage word SHA-256은
`24d3271c4b4af592b97186ce2a0b542c032b4b98c7e93fcb8774a9a687ba9797`다.
q4의 같은 binary kind-union mode 0/1, q5의 같은 binary source-prefix mode 0/1,
q6의 같은 binary boundary-suffix mode 0/1 비교는 각각 완료 계약과 원래 coverage
equality를 통과했다. q4 비교에서는
공개 뒤 kind union의 시간 이득을 관측했지만, q5 비교에서는 일부 구조·정책 카운터의
감소가 총시간 개선으로 이어지지 않았다. q6 비교에서는 같은 board nodes 2,018개와
edges 3,790개를 유지하면서 suffix roots가 31,269→21,158, board work가
167,890→147,650으로 줄었고, 이 완료 pair의 총시간도 줄었다. 각각 한 번의 완료 pair이므로 작은 시간
차이를 안정적인 회귀나 일반적인 개선율로 확대하지 않는다. q3→q4에는 A5와
board root0 생략도 포함되므로 그 버전 간 차이 전부를 kind union 단독 효과로
귀속하지 않는다. 자세한 비교 근거는 `quotient4-eight-comparison-v3.json`과
`quotient5-eight-comparison.json`, `quotient6-eight-comparison.json` 및 별도 실행
기록에 둔다. q9 네 arm의 `quotient9-eight-comparison.json`도 validation/equality를
통과했다. 실행 순서는 표의 edge 0/1/2, collect 0/1이며 ABBA가 아닌 단일 표본이다.
freeze가 밀리초 규모인 이 fixture의 총시간 차이를 closure 단독의 안정적인 개선율로
간주하지 않는다. EDGE=2에서 collection 호출은 1,560,890→11,517, full-subtree 수집은
10,634회였고 실제 정책 상태는 여전히 약 33M이다. owner 추정과 sampled RSS 및 자세한
단일 축 비율은 별도 측정 기록으로 구분한다.

q4 P7P4 실패 시점에는 candidate 253,966, board node 688,308, edge 2,127,017,
suffix root 8,000,000, prefix visit 13,494,008, edge attempt 29,571,418,
work 62,583,912가 기록되었다. q5 P7P4에서는 merge 직전 candidate 352,930,
suffix root 7,297,815, work 57,422,082에 incoming shard의 7,888,073 work를
청구하다 거절되었다. source-pruned prefix는 4,431,529개였지만 전체 family
등록을 끝내지는 못했다. q6 P7P4에서는 candidate 411,520, board nodes 534,090,
edges 1,670,421, suffix roots 3,964,085, work 63,711,712에 incoming shard의
6,062,705 work를 사전 청구하다 거절되었다. H-only suffix로도 전체 등록·정책
평가를 끝내지 못한 결과다. 이 실패들의 등록 모집단이 다르므로 부분 카운터를 같은
완료 workload의 절감률로 비교하지 않는다.

q7 P7P4는 전체 456,459개 candidate와 569,195개 board, 4,361,665개 H suffix root를
생산했다. 이후 finalizer 도중 work **64,000,000**에서 다음 1 work가 거절되었다.
실패 시점의 closure 카운터는 pose checks **14,702,013**, hard-drop hits **708,303**,
reach queries **74,743**, 실제 visited states **14,890,884**, parent boards **36,664**다.
생성 edge는 789,698개이며 이 graph는 부분 상태다. retained/peak는 각각
161,895,456 / 216,687,464 bytes였고 감독 receipt는 `1795327698266632883-5`다.
실제 binary SHA는 `30348d55483bef01f60dade5cfab321b659e2aa19c4fb1df3d8ccff019f4b52c`다.
이 결과는 생산 단계의 반복 제거만으로 finalization까지 충분히 싸지지는 않았음을
보인다. q9는 이 비용 근거에 따라 H−1 층의 이미 증명된 typed edges를 유지한다.

q9 P7P4는 전체 456,459개 후보·569,195개 board·4,361,665개 suffix root와
1,803,660개 edge를 구성하고 freeze를 완료했다. 이때 producer work는 62,477,802다.
이후 kind union work 1,522,198에서 다음 1 work를 청구하다 공통 상한에 도달했다.
source 준비 1.490313838 s, freeze 0.399406499 s, 정책 7.475582441 s가 별도 기록되어
있다. source 준비는 observation evaluator 구간이며 전체 Geometry 시간이 아니다.
kind union의 실패 시 node/edge는 30,376/78,190, member/source visit는
283,015/978,497이다. 단계 진전과 전체 107.122856232 s의 실패 종료를 보존하며,
완료 확률이나 P7P4 speedup을 주장하지 않는다.

q10 P7P4도 전체 456,459개 후보·569,195개 board·1,803,660개 edge·4,361,665개
H suffix root의 등록과 freeze를 끝냈다. producer work는 **62,449,043**이며,
종류 arena에는 **16,373,232 nodes**가 남아 있었다. source 준비 1.288559232 s,
freeze 0.440734120 s 뒤 정책은 **0.348828285 s**에서 work 상한에 도달했다.
kind union과 count 필터의 추가 work **1,550,957**을 합하면 정확히 64,000,000이며
다음 1 work가 거절되었다. count 필터는 root **414,839개 중 390,547개**를 제외했고,
certificate node/edge는 56,383/150,667, alias는 27,665개였다. 당시 kind union의
node/edge는 16,573/24,577, member/source visit는 67,499/126,853이었다.
근거 receipt는 `1795329995908869858-5`다. 많은 root를 제외하고 더 짧은 정책 시간에
중단되었다는 사실은 완료 성능이나 성공확률을 증명하지 않는다. JSON의 `complete=false`
및 `wasm_observation_physical_work_limit`를 그대로 유지하고 8-lock ABBA의 완료 비교와
구분한다.

q5 11-lock의 감독 receipt `1795325923231365760-5`는 timeout을 기록한다.
빈 JSON은 성공확률 0이나 정상적인 미완료 요약이 아니며, 내부 어느 단계가
시간 상한에 도달했는지 이 증거만으로 특정하지 않는다.

q9의 11-lock은 receipt `1795329207758780571-5`에서 419,282 ms 후 SIGKILL 9,
exit 137로 종료되었다. JSON은 0 bytes이고 stderr에는 receipt 경로 외에
source/freeze/policy 내부 단계가 없다. sampled process-group RSS peak는
7,663,796,224 bytes, 감독 soft/hard limit는 8,923,100,775 / 9,914,556,416 bytes이며
memory-pressure events는 0, supervisor termination reason은 null이다. kernel peak와
OOM-kill delta도 null이므로 이 기록만으로 OOM, manager kill 또는 마지막 내부 단계를
특정할 수 없다. 600 s timeout이 발생한 결과도 아니다. 실제 10/11-lock 완주의
확률·coverage는 여전히 pending이며 종료시간을 완료 solver speedup으로 보고하지 않는다.

### 완료한 구현과 검증

- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_factored_actions.rs`, 구현 내용: A1 dormant Frontier의 H=0, 마지막 hidden derivative, terminal 경계를 구현·검증했으며 원래 reveal certificate 전에는 action 전개를 거부한다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_factored_actions.rs`, 구현 내용: A3 hash/new/matches 모두 식 L의 같은 active-to-active labelled row를 사용하며 active 집합·operation label·geometry 권한을 유지한다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/buildup/placement_dependency.rs`, 구현 내용: A4 전체 fresh-bag source 저장소·가중치·checked cardinality로만 식 R의 reveal 하계를 인증하고 12 draw 및 다른 공급 표현의 거절 경계를 검증했다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_factored_actions.rs`, 구현 내용: root0를 유지하고 reveal 하계 미만의 양의 깊이 suffix binding만 제외하며 등록과 worker merge의 n,H,L 일치를 강제한다. q1 registration 실패와 q2 registration/merge 후 정책 확장 실패를 단계별로 계측했다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_board_actions.rs`, 구현 내용: q4 board owner에서는 인증된 L>0일 때 dormant root0 suffix binding도 생략한다. 물리 root와 candidate oracle의 canonical root는 유지하며 L=0 및 factored reuse와 구분한다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy.rs`, 구현 내용: 원래 source·hold·terminal 및 uniform-count 권한을 유지하고 원래 ID domain에 선택 결과를 복원한다. 물리 행동에는 semantic tie를 적용하며 legacy numeric-ID 선택 집합 동일성은 약속하지 않는다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy.rs`, 구현 내용: A5 `for_original_universe`로 사용하지 않는 position index를 생략하며 원래 ID 순서·가중 중복 단어·projected 전 EOF를 보존한다. subset 소비자는 기존 Indexed 경로를 유지하고 q4 focused 회귀로 구분을 확인했다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability.rs`, 구현 내용: Off/Current/Improved와 동일 CSR·query 결과를 검증하고 compile/query/bitmap 메모리 및 P7/P7P4 oracle을 비교했다. 같은 Current–Improved ABBA block의 총시간과 중복 판정 제거를 근거로 Improved를 제품 기본으로 채택했다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/result.rs`, 구현 내용: B5의 내부 whole-family provenance와 final gate를 구현했다. external·partial·truncated 생산은 board closure 권한을 받지 못하며 불완전 결과의 probability/coverage completeness와 not-calculated 출력을 검증했다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_board_actions.rs`, 구현 내용: B5 board/remaining prefix와 물리 edge를 등록·merge·freeze하고 동일 행동의 child, 실제 clear·PC terminal 및 pinned 종류 suffix를 검증한다. reveal 전 경계 전개를 거부한다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_kind_union.rs`, 구현 내용: q4 B6의 exact root-set interning, 모든 같은-kind child의 derivative union, singleton 재사용과 family+union 합산 상한을 구현했다. eager 언어 대조 및 경계 회귀가 포함된 74개 observation focused 검증을 통과했다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/parallel_search.rs`, 구현 내용: 같은 request plan Arc를 native worker에 전달하며 전체 canonical branch 및 producer shard union 이후에만 root 정책 평가를 허용한다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/parallel_worker.rs`, 구현 내용: worker 시작에 같은 plan을 bind하고 local 정책 값이 아닌 물리 관계와 종류 arena의 suffix root를 coordinator에 넘긴다.
- [x] 수정 파일: `docs/research/see7-quotient-pruning-proof-2026-11-22.md`, 구현 내용: split O까지 포함하는 catalog lift, `h+d≤H_b`, source-relative B5 product 동치, q4 kind union·원래 index 생략·root0 생략, q5 공급 제한·productive filter, q6 H-only suffix, q7의 positive-action/partial-query와 q9 층별 혼합 따름정리를 실제 함수에 연결했다. B10 full-count 수집, B11 실제 관측 개수 필터 및 B12 prefix word mask의 exact key·경로·사전 work·peak·fallback 증명을 추가했다. q9 완료 8-lock, q10 focused 검증과 P7P4 실패, q11 제품 연결 후 미검증 및 q12 별도 작업 트리 상태를 구분한다.

### q5/q6 구현·focused 검증과 완료한 비교

- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs`, 구현 내용: 인증된 전체 fresh bag과 checked `c₀+H+E≤7`에서만 종류별 1개 및 초기 held 종류 1개 추가 상한을 같은 BoardObservationPlan Arc에 넣는다. q5의 78개 observation focused 검증 및 release 빌드를 완료했다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_board_actions.rs`, 구현 내용: B7의 reverse productive pass로 cap 위반 prefix와 retained H 경로가 없는 prefix를 분리해 제외하고 남은 실제 물리 경로만 등록한다. O/O/I 실제 geometry, 전체 dead family, 초기 held 복구 회귀를 통과했으며 실제 8-lock mode 0/1에서도 같은 coverage를 확인했다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_board_actions.rs`, 구현 내용: q6 B8의 H-only binding, 조기 reveal 뒤 raw physical node 유지, H=0/H=n 및 EOF held-release 경계를 구현했다. 관련 권한을 보정한 fixture를 포함해 80개 observation focused 검증과 release 빌드를 완료했으며 실제 8-lock boundary mode 비교에서도 같은 coverage를 확인했다.
- [x] 수정 파일: `docs/research/see7-pruning-measurements-2026-11-22.md`, 구현 내용: 같은 q4 binary의 kind-union mode 0/1 완료 대조와 원래 ID equality를 기록했다. q3/q4 전체 변경 비교와 구분한다.
- [x] 수정 파일: `docs/research/see7-pruning-measurements-2026-11-22.md`, 구현 내용: q5 8-lock source-prefix mode 0/1의 완료 계약·coverage equality와 총시간 개선 미관측을 기록했다. q5 P7P4의 work-limit 실패와 11-lock 제한 probe의 timeout을 완료 결과와 구분했다.
- [x] 수정 파일: `docs/research/see7-quotient-pruning-proof-2026-11-22.md`, 구현 내용: q6 release 및 실제 8-lock boundary mode 0/1의 완료·동등성 근거와 P7P4의 계속된 work-limit 실패를 §9에 반영했다. 상세 측정 문서와 동일 artifact를 참조한다.

### q7 구현과 검증

- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_board_actions.rs`, 구현 내용: 기본 0인 B9 실험의 complete-family legal-edge finalizer를 구현했다. retained board와 모든 full-field pose, exact child clear, hard-drop 양성 및 요청한 모든 pose의 도달성 결과로 edge를 재구성하고 실패 후 freeze를 거부한다. 전체 1,104 passed의 검증 범위에 포함된다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability.rs`, 구현 내용: 같은 request authority의 desired-lock query가 exhaustive 또는 all-desired-positive cache만 재사용하도록 구현하고 실제 visited states를 계상한다. 같은 nominal ID의 ordered-kick 재배열에 따른 양성·음성 및 cache 재사용 회귀를 포함한다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs`, 구현 내용: profile 설정은 EDGE=1에서만 요청의 conditioned 값을 보존하며 수행하도록 guard를 수정했다. 이후 observation focused 81 passed를 확인했고 guard 수정 뒤 전체 재실행과 구분한다.
- [x] 수정 파일: `docs/research/see7-quotient-pruning-proof-2026-11-22.md`, 구현 내용: q7 release 완료 및 실제 P7P4의 전체 후보 생산 뒤 finalizer work-limit 실패를 기록했다. 실제 pose/query/visited 비용을 q9 hybrid 층 선택의 근거로 남겼고 완료된 정책 결과와 구분한다.

### q8/q9 구현·focused 검증과 완료한 8-lock 비교

- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy.rs`, 구현 내용: B10 shortcut을 기존 accepting 처리와 count 0 반환 뒤에 적용했다. 현재 원래 subtree 전체 count와 고정 horizon에서만 ID를 직접 수집하고 호출·shortcut 횟수 두 지표를 기록한다. `dcf06bc` focused 검증과 실제 EDGE=2의 collection 0/1 비교를 완료했다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy_tests.rs`, 구현 내용: 비균등·0 가중치의 동일 sequence별 원래 ID, 미공개 미래, 현재 target 실패/미래 target 성공, partial information set 및 variable horizon의 추가 회귀를 실행했다. 초기 terminal-target fixture 생성 오류를 수정한 뒤 기본 84개·parallel/local A/B 86개 관측 검증을 통과했다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_board_actions.rs`, 구현 내용: q9 EDGE=2가 depth+1<H에서만 전역 legal-edge를 재구성하고 H−1 typed edge를 유지하도록 공통 should_rebuild 판정을 적용했다. 같은 Plan·상한·완료 권한을 유지하고 H0/H1에서는 pose 준비도 생략한다. `dcf06bc` focused 검증과 실제 8-lock 세 EDGE mode의 완료·원래 coverage 동등성을 확인했다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_board_actions.rs`, 구현 내용: 세 EDGE mode의 physical paths, merge 뒤 마지막 층 불변, finalizer 우회 거절 및 H0/H1의 재구성 비용 0 회귀를 `dcf06bc` 관측 검증에서 확인했다. 이 범위를 전체 플랫폼 qualification이나 10/11-lock 완주로 확대하지 않는다.
- [x] 수정 파일: `docs/research/see7-quotient-pruning-proof-2026-11-22.md`, 구현 내용: q9 P7P4의 등록·freeze 이후 정책 work-limit과 11-lock의 SIGKILL을 기록했다. 후자는 단계·OOM 여부가 확인되지 않았으며 600초 timeout 또는 정상 확률 0으로 표시하지 않는다.

### q10 구현·focused 검증과 미완료 P7P4 실측

- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy.rs`, 구현 내용: 실제 원래 관측 깊이와 fixed horizon/terminal-target 경계에서만 사용 가능 공급 count를 제공한다. projected draw·materialized EOF·usable held를 구별하며 결정된 숨은 미래만으로 count hook을 허가하지 않는다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_kind_union.rs`, 구현 내용: immutable kind arena의 모든 간선에 대한 sparse uniform multiset 증거, frozen board+count alias, unknown/cap fallback, 같은 remaining의 명시적 empty reject와 공통 work·owner 계상을 연결했다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_board_actions.rs`, 구현 내용: 기본 off인 필터는 union이 켜진 H frontier에서만 요청하며 이미 변환된 kind/union과 unfiltered alias를 분리한다. 공급 밖 raw 언어 equality나 CountAll/evidence 권한을 부여하지 않는다.
- [x] 수정 파일: 관측 policy·kind union·board view의 테스트 파일, 구현 내용: 여섯 추가 회귀와 기존 Hold 경계를 확인했다. 첫 fixture의 trie 준비 누락을 수정한 뒤 `d17c26e`에서 92개 focused 검증을 통과했다. source 검토·전칭 필요조건 증명과 실제 실행 검증을 각각 기록한다.
- [x] 수정 파일: `docs/research/see7-quotient-pruning-proof-2026-11-22.md`, 구현 내용: q10 P7P4의 93.529510444 s 실패와 전체 등록·freeze 뒤 producer 62,449,043 + 정책 1,550,957 work의 공통 상한을 기록했다. 414,839개 중 390,547개 root 제외를 완료나 확률 0으로 해석하지 않는다.
- [x] 수정 파일: `docs/research/see7-pruning-measurements-2026-11-22.md`, 구현 내용: q10 같은 바이너리 8-lock ABBA에서 원래 coverage가 일치하고 평균 36.9037816145→10.6043791735초임을 확인했다. 실제 policy states/action checks는 줄었지만 kind work는 1,736,625→23,741,330으로 늘어 작업 상한 여유와 총시간을 구분한다. 10/11-lock 완주를 이 결과로 대체하지 않는다.

### q11 구현·실행 검증과 완료 비교

- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_prefix_masks.rs`, 구현 내용: n≤11의 exact 3-bit operation sequence와 owner n/H/cap를 사용하는 512-entry bounded cache, compile-time T 표, common depth mask, scalar와 같은 valid/cap-invalid word, slot ticket 및 부분 cache fallback을 구현했다. helper의 7개 회귀를 포함한 observation 101개 검증을 통과했다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_board_actions.rs`, 구현 내용: 기본 off인 prefix mask를 조건부 word 처리와 기존 reverse productive pass에 연결했다. helper의 사전 예산과 오류·취소 후 work delta, worker merge와 owner retained/peak를 반영했으며 독립 읽기 감사를 마쳤다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/result.rs`, `crates/clearra-core-executor/examples/observation_dependency_bench.rs`, 구현 내용: `[lookups,hits,misses,full_fallbacks,words,work]` 여섯 지표와 benchmark v9의 `prefix_mask_mode`를 연결했다. 기존 board work에 포함된 추가 비용을 결과에서 다시 청구하지 않는다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_prefix_masks.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_board_actions.rs`, 구현 내용: helper 7개 및 등록 on/off·merge·취소/예산을 실행 검증했다. 소스 증명과 실행 통과를 구분하고 실패를 상한 완화로 해결하지 않았다. 최종 feature gate는 별도로 실행한다.
- [x] 수정 파일: `docs/research/see7-pruning-measurements-2026-11-22.md`, 구현 내용: 같은 q11 binary의 8-lock PREFIX_MASK ABBA에서 완료·원래 coverage 일치, 평균 11.7158377735→11.8070351215초를 기록했다. P7P4는 5,913,906 prefix를 생략하고 helper work 4,416,768을 실제 청구했지만 두 모드 모두 미완료다. worker 배분에 영향을 받는 전체 work 차이를 단순 R−Q로 단정하지 않는다. 기본 off를 유지한다.

### q12/q13 통합과 실행 검증

- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/piece_order_language.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_physical_actions.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_board_actions.rs`, 구현 내용: B13 coordinator subset import를 기존 owner/ID를 바꾸지 않는 append-only map으로 구현했다. 5개 subset import 검증과 실제 board merge의 coverage/physical work 대조를 통과했다. worker-exit GC는 미구현이다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy_tests.rs`, 구현 내용: B14의 bounded raw DP memo와 실패·scope·Hold·original ID 경계를 구현했다. 새 7개를 포함한 observation 110개 검증을 통과했다. collect replay와 기존 canonical memo는 유지한다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/result.rs`, `crates/clearra-core-executor/examples/observation_dependency_bench.rs`, 구현 내용: 두 옵션의 기본 0, import 13개·raw memo 6개 계측과 schema v10을 연결했다. 기존 owner가 보유한 메모리에 중복 청구하지 않고 map/old-new 공존분만 추가했다.
- [x] 수정 파일: `docs/research/see7-pruning-measurements-2026-11-22.md`, 구현 내용: full source SHA를 가진 q13b에서 q13 ABBA와 별도 q12 한 arm의 완료·원래 coverage·단계 비용·import peak를 비교했다. q13 평균 시간 악화와 P7P4 work-limit을 기록했으며 q12 한 arm을 독립 ABBA로 표현하지 않는다. 최초 short-SHA 빌드는 provenance 실패 자료로 보존하고 정식 비교에서 제외했다.

### q14/q15 source 판정과 경로 축약

- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/buildup/placement_dependency/fresh_bag_observation.rs`, 구현 내용: B15의 first-six distinct 및 전체 종류별 2개 상한을 필요충분 판정으로 구현했다. 실제 Hold automaton 구성 35,126개와 별도 원래 bag product 대조를 포함한 관측 120개 검증을 통과했다. serial/general/evidence 경로와 strict A/B 계약은 기존 경로에 남는다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_physical_actions.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/piece_order_language.rs`, 구현 내용: owned candidate의 H-only exact suffix 생성과 typed Compiled/EmptyRelation 결과를 연결했다. 후보의 oracle coverage를 만들어내지 않고 원래 Ω의 최종 정책만 계산한다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_prefix_source_filter.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_prefix_orders.rs`, 구현 내용: B16의 source-qualified active closure를 원래 immutable proof와 묶었다. source mask의 7개 회귀를 통과했고 full/H4 모든 보유 suffix의 독립 단어 필터 대조와 실제 source-compiler 작업량을 포함한다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_board_actions.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_physical_actions.rs`, 구현 내용: typed source 경로에서만 q5/q11 중복 cap을 생략하고 filtered child를 suffix와 물리 등록에 같이 적용했다. raw route, worker merge, 실패·취소와 원래 ID 경계를 포함한 관측 124개 검증을 통과했다.
- [x] 수정 파일: `docs/research/see7-pruning-measurements-2026-11-22.md`, 구현 내용: q15 같은 바이너리 source-filter 8-lock ABBA의 완료·coverage 일치와 평균 비율 0.9344308135557023을 기록했다. 실제 10/11-lock은 각각 정책 work/중앙 node 한도로 미완료이며 실패시간을 완료 speedup으로 비교하지 않았다. source-filter 기본 0을 유지한다.

### 완주 확인과 미적용 후속 후보

- [x] 수정 파일: `docs/research/see7-pruning-measurements-2026-11-22.md`, 구현 내용: 같은 q9 binary에서 `(EDGE,COLLECT_FULL)=(0,0),(1,0),(2,0),(2,1)`의 실제 8-lock을 완료하고 Rust comparator로 원래 coverage와 완료 계약을 확인했다. 순차 단일 표본과 별도 freeze/policy 비용을 유지하며 q7의 실패시간을 완료 대조로 사용하지 않는다.
- [ ] 수정 파일: `docs/research/see7-pruning-measurements-2026-11-22.md`, 구현 내용: 실제 10/11-lock full see-7을 완료하고 원래 공급·관측·확률 완전성, source-pruned/dead-source prefix, board edge/roots, source·freeze·policy 단계 시간과 worker/coordinator peak를 기록한다. q5의 work-limit·timeout과 완료된 8-lock 결과를 구분한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_factored_actions.rs`, 구현 내용: A2가 여전히 필요한 경우에만 제한 깊이의 exact action signature를 추가한다. A1/A3/A4 및 B5의 구현 완료와 구분한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_factored_actions.rs`, 구현 내용: B3 별도 score memo를 적용한다면 완료 권한 아래 board/depth value class만 공유하고 raw residual·frame 및 행동 재평가를 유지한다. 현재 채택한 B5 board closure의 필수 미완료 단계는 아니다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/piece_order_language.rs`, 구현 내용: C를 적용한다면 같은 arena/깊이의 bounded directed simulation을 제공하고 비교 예산 초과에서 원래 root를 유지한다. 현재는 미적용 설계다.
- [x] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_word_bitmap.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_kind_union.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy.rs`, 구현 내용: 실제 관련 draw가 공개된 r≤6 공급의 exact membership을 q20으로 구현하고 161개 관측 통합 검증을 통과했다. 일반 r≤7 producer bitmap 교체와 구분하며, 64M/8M 상한 내 10/11 완주를 이 항목만으로 주장하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/piece_order_language.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_board_actions.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_physical_actions.rs`, 구현 내용: q12와 별개로 worker 원본 arena를 실제 축소할 필요가 있으면 retained-root closure·모든 ID 소비자·product cache 정리·원자적 owner 교체·실제 재바인딩 work와 공존 peak를 검토한다. pinned base가 recycle threshold를 계속 넘는 현상을 진단하며, threshold 변경이나 주기적 GC를 무검증 채택하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy.rs`, 구현 내용: trie 비용이 실제 병목이면 D source DAG를 기존 standard-bag 전이·suffix count와 연결하며 sparse/가중 일반 공급은 기존 표현을 유지한다. 현재는 미적용 설계다.

## 10. 실제 11-lock이 요구하는 추가 구조 변경

q15의 아홉 worker는 geometry 후보 414,005,053개를 처리하고 source-positive
2,008,622개를 생산했다. coordinator는 두 번째 suffix import 이후
`wasm_observation_physical_node_limit`으로 종료했다. freeze와 최종 정책은 실행하지
못했다. 따라서 q14의 worker 내부 실패보다 뒤 단계까지 진행했지만 full see-7 완료는
아니다. 이 실행의 578.288882526초를 완료 시간 개선으로 비교하지 않는다.

### 10.1 생산 work의 실행 기록에 근거한 하한

아래 첫 계산은 q15 producer의 기록이다. q18은 생산 알고리즘을 바꾸었으므로
q15 하한을 그대로 적용하지 않고 다시 실행했다. q18의 완료된 아홉 worker
최종 합계는 **71,396,217**이며, 별도 Rust v2 추출 결과와 측정 보고서 §3.38에서
확인했다. 이것도 병합·freeze·policy 전부터 64M을 넘는다. 따라서 후속 policy와
H-class를 적용하더라도 producer를 추가로 줄여야 한다는 결론은 q18에서도 유지된다.

각 worker가 `candidate_count=131072`에서 남긴 아홉 값은 다음과 같다.

$$
8{,}720{,}092+7{,}976{,}355+7{,}853{,}767+8{,}503{,}762+
7{,}666{,}783+8{,}690{,}303+10{,}122{,}558+10{,}120{,}749+
8{,}209{,}054=77{,}863{,}423>64{,}000{,}000.
$$

`run_branch_worker_inner`는 worker당 workspace 하나를 만들고 모든 branch task에서
재사용한다. shard를 꺼내는 것은 종료 시 한 번이며, symbolic cache 재활용은
BoardActionFamily의 counter를 초기화하지 않는다. `register_inner`의 candidate 수와
work는 단조 증가하고 해당 milestone을 worker당 한 번만 출력한다. coordinator의
`merge_inner`는 첫 shard부터 `source.metrics.work+1`을 누적한다. 따라서 위 합은
남은 후보·실제 merge·freeze·policy를 모두 제외한 **현재 생산 표현의 하한**이다.

독립 Rust 도구 `worker_work_bound.rs`가 worker 수·완료 counter의 partition과 합을
검증하고 `quotient15-eleven-work-lower-bound.json`을 생성했다. 이것은 다른 알고리즘의
불가능성 정리가 아니다. 그러나 현재 worker 생산을 그대로 둔 채 policy만 바꾸거나
merge 뒤 H 노드만 합치는 변경은 기존 64M 안의 full 11을 완성할 수 없다.

### 10.2 H 경계의 mutable 병합이 부정확한 이유

현재 두 보드의 언어가 $L_x=L_y=\{u\}$라고 해도 생산 중에는 최종 언어가 아니다.
이때 하나로 합친 뒤 다음 candidate가 $x$에만 $v$를 추가하면 $y$도 근거 없이 $v$를
수용하게 된다. 같은 multiset은 단어 순서 언어의 equality를 증명하지 않는다.
immutable set payload를 공유하는 것은 안전하지만 보드별 construction identity는
남으며, 별도 증명 없이 이 alias를 node 한도에서 제외할 수는 없다.

q15는 worker 생산을 끝냈으므로 completion barrier 뒤 **batch H-frontier quotient**는
검토할 수 있다. 같은 H 보드에 대한 모든 shard의 rebound root를 먼저 완전히 합치고,
완성된 exact root-set이 같은 경계를 immutable class로 공유한다. 마지막 physical
edge는 원래 child board의 place/clear 증거를 유지한다. 다만 기존 shard별 즉시 merge
API를 바꾸고, 모든 owner·rebind map·staging·destination 공존 peak와 추가 work를
계상해야 한다. 실제 보드 사이의 완성 root-set 중복률은 아직 측정하지 않았다.
그리고 이 변경만으로 §10.1의 worker 생산 하한이 사라지지는 않는다.

### 10.3 원래 symbolic compile 이전의 source mask는 시간 개선 후보

현재는 raw physical relation을 만든 다음 B15의 존재성 검사와 B16의 mask를 적용한다.
동일 certificate 아래 깊이≤6의 중복-kind prefix와 깊이>6의 distinct-kind 수가 6보다
작은 prefix를 `UpperFrameGuards::compile`의 frame 분할 전에 제외할 수 있다. 기존
optimistic/final closure와 같은 exact reachability를 유지하면 B16과 동일한 최종
source-qualified graph를 더 적은 raw lane 계산으로 만들 수 있다.

native catalog의 operation 순서는 piece와 cells 순서이므로 같은 exact 종류 layout의
mask는 공유할 수 있다. n11/count≤2에서 layout은 최대 77개이며, 각 32-word payload의
합은 19,712 bytes다. 두 multiplicity 형태의 유효 subset 수는 terminal 포함
984/2048 또는 697/2048이다. 이는 계산에서 제외할 raw lane의 조합적 수이며 실제 시간
감소율이나 cap 해결을 뜻하지 않는다. 결과에는 source-qualified owning 권한을 붙이고
무제한 raw `ExactPrefixRelation`로 재발급하지 않아야 한다.

이 선행 mask는 아직 구현하지 않았다. q15와 최종 board graph 및 등록 방문이 같으므로,
full 11의 우선 과제는 **후보별 생산 반복을 실제로 제거하는 family 단위 공유**와
완성된 H 언어의 추가 압축을 함께 설계하는 것이다. 기존 상한을 바꾸거나 이미 수행한
worker work를 합산에서 빼는 것은 이 과제의 해결로 간주하지 않는다.

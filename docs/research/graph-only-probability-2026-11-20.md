# 큐 열거 없는 확률과 배치 순서 그래프 생략

작성일: 2026-11-20 KST. 앞선 [P7P4 검증](placement-graph-p7p4-2026-11-20.md)을 잇는다.

## 1. 계산 대상을 먼저 고정한다

고정된 canonical tiling T의 concrete placement 집합을 V, 각 placement의 미노 종류를 k(p), 원래 공급 큐의 유한 universe를 Ω라고 하자. 같은 종류라도 다른 셀 집합을 차지하는 placement는 서로 다른 p다. L(T)는 물리적으로 실행 가능한 종류 문자열들의 집합이다. H(q)는 큐 q와 지정된 공급·hold·종료 정책에서 허용되는 출력 문자열들의 집합이다. 초기 hold뿐 아니라 hold 허용 여부, terminal/lookahead projection도 고정한다.

정확한 성공 큐 집합과 확률은 다음과 같다.

$$
Q_T=\{q\in\Omega\mid H(q)\cap L(T)\ne\varnothing\},\qquad
P_T=\sum_{q\in Q_T}\mu(q).
$$

표준 가방의 고정 길이 prefix를 완전하게 다루면 원래 큐들은 균일하므로 P_T=|Q_T|/|Ω|다. P7P4의 universe 크기는 7!×7P4=4,233,600이다. 어떤 한 큐가 여러 배치 순서, 여러 동일 kind placement 대응, 여러 hold 행동으로 성공하더라도 확률에서는 한 번만 센다. CountAll의 실행 variant 수는 별도 계산이다.

전체 성공 확률은 P(∪_T Q_T)다. 해별 확률을 더해서 전체 확률을 만들 수 없다. 각 해의 scalar만 보관하면 두 해의 성공 큐가 얼마나 겹치는지 복원할 수 없다. 예를 들어 크기 2인 두 집합의 합집합 크기는 2, 3, 4 중 어느 것이든 가능하다.

현재 `StandardBagCoverage::cover_language`는 이미 큐를 개별 배열로 만들지 않고 canonical decision DAG에서 정확한 covered count를 얻는다. `Include` 결과는 그 root를 해별 비트셋으로 물질화한다. 따라서 이번 개선의 핵심은 기존 확률 엔진을 복제하는 것이 아니라 그 앞의 물리 상태 그래프를 안전하게 생략하는 것이다.

## 2. 생략할 수 있는 세 종류의 상태

| 계산 대상 | 기존 역할 | 생략 또는 축약 조건 |
| --- | --- | --- |
| 개별 입력 큐 | 해별 queue export와 coverage 비트셋 | scalar만 필요하면 symbolic root의 정수 집계로 대체 |
| 물리 배치 순서 그래프 | 실행 subset별 ILC·grounding·reachability 검증 | 가능한 모든 순서를 정확히 나타내는 구조적 certificate가 있을 때 생략 |
| 수학적 residual 상태 | 순서 제약과 bag·hold의 결합 | 독립/직렬 분해, 동일 kind의 증명된 대칭, 완전한 suffix 수락으로 축약 |

물리 graph를 없애더라도 일반적인 순서 제약과 bag·hold를 계산하는 residual 상태까지 반드시 없어지는 것은 아니다. 어떤 작은 dependency graph도 상수 시간 확률 공식으로 바꿀 수 있다는 보장은 없다. 일반 poset의 linear extension 수 계산은 #P-complete다. 이 사실은 구조적인 쉬운 부분을 찾아 분해할 필요성을 설명하며, 이번 고정 크기 PC 입력의 실행 시간을 직접 예측하지는 않는다.

## 3. 순열을 수식으로 처리할 수 있는 범위

### 3.1 홀드 없는 한 가방, 서로 다른 종류

서로 다른 k개 종류의 placement가 정확한 poset P를 이루고, fresh 7-bag에서 처음 k개를 모두 배치하며 hold가 없다고 하자. 성공 queue와 linear extension이 일대일로 대응하므로

$$
P_T=\frac{e(P)}{7P_k},\qquad 7P_k=\frac{7!}{(7-k)!}.
$$

그 뒤의 가방 suffix를 관측만 하고 배치에 사용하지 않으면 분자·분모에 같은 suffix 가짓수가 곱해져 소거된다. 앞의 k개 중 불필요한 종류를 임의로 건너뛸 수 있다는 뜻은 아니다.

P와 R의 모든 원소가 서로 독립인 parallel composition에는

$$
e(P\parallel R)=\binom{|P|+|R|}{|P|}e(P)e(R)
$$

가 성립한다. P의 모든 원소가 R보다 앞서는 series composition에는

$$
e(P;R)=e(P)e(R)
$$

가 성립한다. 따라서 series/parallel로 분해되는 구조는 각 placement subset을 방문하지 않고 작은 산술 식으로 정확히 센다. 7종류 모두 독립이면 7!/7!=1이고, 3개가 한 chain을 이루고 나머지 4개가 독립이면 (7!/3!)/7!=1/6이다. 이 예시는 순서 구조의 산술 예시이며, 특정 물리 필드가 그런 구조임을 따로 증명해야 한다.

이번 산술은 별도 실험 API `count_distinct_no_hold_one_bag_poset`와 `--scalar-demo`에서 실행한다. 실제 candidate bypass는 hold와 bag 경계를 유지하기 위해 기존 `StandardBagCoverage`를 계속 사용한다. 모든 제품 확률 경로가 이 단순 공식으로 바뀐 것은 아니다.

### 3.2 같은 종류를 무조건 factorial로 나누면 안 된다

I_a, I_b, T 중 I_a<T만 강제되는 예를 생각하자. concrete 순서는 세 개지만 종류 문자열은 IIT와 ITI 두 개다. concrete 수 3을 2!로 나누면 1.5가 되어 잘못된 결과를 준다. 종류 문자열마다 대응하는 concrete 순서 수가 일정하지 않기 때문이다.

이번 직접 언어 컴파일은 같은 kind이고, 전체 predecessor와 successor가 같으며 서로 incomparable인 placement들만 하나의 count class로 합친다. 같은 class의 구성원을 서로 바꾸어도 남은 제약과 출력 kind가 같다는 것이 근거다. 셀 좌표가 다른 placement를 처음부터 하나로 지우거나, 같은 kind라는 이유만으로 class를 합치지 않는다.

class i의 multiplicity를 m_i라고 하면 concrete subset의 최대 2^n개 상태 대신 count vector의 최대 ∏_i(m_i+1)개를 다룰 수 있다. poset 제약으로 방문 가능한 count vector는 이보다 적을 수 있다. 이것은 물리 위치의 동등성이 아니라 **증명된 순서 언어 내부에서의** 동등성이다. CountAll은 이 quotient를 사용하지 않는다.

어떤 O가 항상 놓일 수 있다는 사실만으로 O를 독립 순열 factor로 처리할 수는 없다. O를 먼저 놓는 것이 J의 후속 가능성을 바꾸면 완성 언어에서는 의존성이 남는다. 자유로운 interleaving을 묶으려면 양쪽 순서를 교환해도 이후 성공 언어와 필요한 clear 조건이 보존되어야 한다. 대표 순서 하나만 남기는 reachability용 reduction도 원래 큐의 확률을 자동으로 보존하지 않는다.

### 3.3 홀드와 가방 경계를 넣으면 무엇이 달라지는가

홀드는 성공 출력 순서에서 원래 입력 큐로 가는 대응을 일대일이 아니게 만든다. 또한 P7P4에서는 한 종류가 두 번 등장하고, bag 경계가 순서를 제한한다. 따라서 e(P)를 단순히 7!×7P4로 나누는 공식은 적용할 수 없다.

이 경우 계산 상태는 (남은 종류 언어, 공급 cursor, 정확한 bag remainder, hold)다. 같은 입력 토큰 아래에서 가능한 여러 실행·hold 분기는 합집합으로 합쳐야 한다. 원래 큐의 다음 토큰별 가지는 서로 배타적이므로 그 가지들을 합산할 때만 단순 덧셈이 정당하다. 모든 남은 큐를 수락한 suffix는 남은 bag permutation 수를 한 번에 더할 수 있다.

이는 deterministic decision DAG와 weighted counting의 조건과 맞닿아 있다. OR의 자식이 겹치면 scalar를 더할 수 없고, AND의 자식이 변수·공급 자원을 공유하면 scalar를 곱할 수 없다. 독립적인 물리 component라도 같은 bag과 hold를 공유하면 확률적으로 독립인 component라는 뜻이 아니다.

### 3.4 P7P4에서도 순열 전체를 소거할 수 있는 특별한 경우

처음 7개는 한 가방 전체, 다음 4개는 새 가방의 ordered prefix이고, 이 11개 중 10개를 배치하며 빈 hold에 최대 1개를 남길 수 있다고 하자. target multiset m의 **모든 종류 순서**가 허용된다면 성공 여부는 공급 개수가 m을 포함하는지만으로 정해진다. 유일한 잉여 미노를 hold에 저장하고 나머지를 공급 순서대로 배치하면 되기 때문이다.

각 m_k가 0,1,2 중 하나이고 합이 10일 때 두 번 필요한 종류 집합을 R, 그 크기를 r이라고 하자. 다음 가방에서 뽑은 4종류 집합 S가 R을 포함해야 하므로

$$
P=\frac{\binom{7-r}{4-r}}{\binom{7}{4}}\quad(r\le4).
$$

첫 가방의 7! 순서와 다음 4종류의 4! 순서는 분자·분모에서 소거된다. r=3이면 7종류 모두와 추가 3개를 쓰므로 4/35, r=4이면 6종류와 그중 추가 4개를 쓰므로 1/35다. 어떤 종류가 3번 필요하거나 r>4면 0이다. 홀드가 없으면 r=3의 값은 1/35, r=4는 0으로 달라진다.

이 공식은 이번 코드에 추가한 별도 fast path가 아니라 다음 압축을 위한 수학적 기준이다. 순서 제약이 있는 실제 tiling에서는 같은 multiset의 **상한**이며, 그 값으로 실제 확률을 대체할 수 없다. 추후 lower-language가 이 상한에 도달했다면 §6.2의 포화 증명을 적용할 수 있다.

현재 기본 opening query의 계약은 위 모델과 일치한다. `preset/opening_preset.rs`는 10개 배치와 빈 hold를 전달하고, `search_problem.rs::resolve_supply_window`는 source 길이 11, `projects_unplaced_lookahead=false`를 만든다. `StandardBagCoverage::solve`는 10개 배치 언어의 terminal에서 수락하므로 마지막 한 개는 미사용 lookahead 또는 held surplus가 될 수 있다. 반면 no-hold 기본 source 길이는 10이다. 이 경우 분모는 7!×7P3=604,800이며 r=3의 성공 큐 수는 17,280이다. explicit source 길이를 11로 고정한 경우와 count 값을 혼동하지 않는다.

### 3.5 같은 종류의 선행관계까지 없앨 수 있는 정확한 조건

유한 비순환 exact poset P에서 모든 concrete operation을 한 번씩 사용한다고 하자. 다음 동치는 성립한다.

$$
L(P)=\operatorname{MultisetWords}(m)
\iff
\text{서로 다른 kind 사이의 선행관계가 없다.}
$$

충분성은 kind별 내부 poset의 위상정렬을 하나씩 고정하면 보인다. 원하는 종류 문자열에서 해당 kind의 j번째 등장에 그 kind 내부의 j번째 operation을 대응시키면 모든 선행관계를 만족한다. 필요성은 서로 다른 종류 A,B의 a<b 관계가 있을 때 모든 B를 모든 A보다 먼저 둔 문자열이 반드시 실패한다는 사실로 보인다.

따라서 같은 kind의 chain이나 복잡한 poset이 남아 있어도 그 순서를 확률 상태에 넣을 필요가 없다. §3.2의 twin quotient보다 강한 특별한 단순화다. 구현 시에는 exact certificate를 얻은 뒤 각 p에 대해 `predecessors[p] & !same_kind_mask[p] == 0`인지 확인하면 된다. 물리 가능성의 필요조건만 나타내는 poset에는 이 결론을 적용할 수 없다.

### 3.6 P7P4의 성공 집합을 35비트로 정확히 합치는 범위

§3.4–3.5의 조건을 만족한 해에서는 성공 여부가 두 번째 가방의 4종류 집합 S에만 의존한다. 가능한 S는 35개이고, 각각에 속한 원래 큐 수는 7!×4!=120,960으로 같다. 한 해를 다음 35-bit mask로 표현할 수 있다.

$$
M_T=\{S\subseteq\{I,O,T,L,J,S,Z\}: |S|=4,\ R_T\subseteq S\}.
$$

r=3이면 4비트, r=4이면 1비트가 켜진다. 이 특별한 family F의 합집합은

$$
M_F=\bigvee_{T\in F}M_T,\qquad
P_F=\frac{\operatorname{popcount}(M_F)}{35}
$$

로 정확하게 구할 수 있다. 개별 큐·첫 가방 순열·둘째 가방 내부 순열을 생성하지 않으며, scalar를 더하는 오류도 없다. 이는 §1의 scalar-only union 불가능성을 무시한 것이 아니라 원래 사건을 충분히 표현하는 더 작은 집합을 보관한 것이다.

이미 증명한 lower queue coverage가 §3.4의 upper set에 포화된 해도 같은 mask로 표현할 수 있다. 이 경우 실제 종류 언어 전체가 자유롭다는 것까지 증명할 필요는 없다. 다만 일반적인 constrained candidate의 기존 symbolic root와 섞을 때는 35개 사건을 해당 root에 정확히 lift하거나 혼합 표현의 교집합·합집합을 계산해야 한다. 원래 큐 순서에 민감한 부분을 무조건 35개로 축약하면 안 된다. 빈 hold, fresh P7P4, 10개 배치, 균일 표준 가방이라는 계약도 함께 보존한다. 이 35-bit 표현은 후속 설계이며 이번 실행 바이너리에는 포함하지 않았다.

## 4. 물리 그래프를 만들지 않는 exact certificate

### 4.1 원래 행과 실행 집합

E는 이미 실행한 concrete operation 집합이다. 원래 행 r의 빈 칸을 채우는 operation 집합을 C_r라고 하면, 완성 대상 행에 대해

$$
D(E)=\{r\mid C_r\subseteq E\},\qquad
B(E)=\operatorname{Compact}_D\left(B_0\cup\bigcup_{p\in E}\operatorname{cells}(p)\right).
$$

실제 코드에서 이미 가득 찬 초기 행, target 밖의 셀, 겹치는 placement, 불완전한 target은 별도 검증한다. certificate의 입력은 final board가 비는 완전한 candidate다. 현재 지원 범위는 폭 1–10, 높이 1–6, operation 1–15다. partial tiling X와 실행 집합 E를 혼동하지 않는다.

### 4.2 필요 선행 관계를 뽑는다

placement p와 가능한 삭제행 frame D에서 실제 lock은 다음 중 하나의 grounding witness를 가져야 한다.

1. floor 또는 초기 board가 받침을 제공한다.
2. 이미 실행한 다른 placement q의 살아남은 셀이 받침을 제공한다.

D의 모든 삭제행 contributor는 이미 실행되어야 한다. 이를 R_D라고 하자. 첫 경우의 required set은 R_D, 두 번째 경우는 R_D∪{q}다. 가능한 모든 D·ILC realization·grounding witness의 required set을 교집합하면 p보다 반드시 먼저 실행되어야 할 operation 집합을 얻는다.

이 단계에서는 실제로 도달할 수 없는 witness를 일부 포함해도 필요조건이 약해질 뿐이다. 가능한 실제 witness를 빠뜨리면 필요조건이 지나치게 강해질 수 있으므로 허용되지 않는다. 실제 E,D와 supporter q에는 R_D⊆R_D∪{q}⊆E이고 D(R_D)=D(E)=D가 성립한다. 따라서 단조성으로 D(R_D∪{q})=D이므로, 구현의 이 frame 일치 필터가 실제 witness를 제거하지 않는다. 추출한 관계는 transitive closure를 취한다. cycle 또는 유효 witness가 없는 경우도 이번 prototype에서는 기존 exact 경로로 넘긴다.

### 4.3 모든 허용 순서가 물리적으로 실행됨을 증명한다

필요조건만으로는 exact graph가 아니다. 그 poset에서 p가 다음에 올 수 있는 모든 이상집합 E를 직접 방문하지 않고 검증한다.

고정된 p,D에 대해 다음 경계를 만든다.

- E_min: p의 모든 predecessor와 D의 contributor를 포함하고 predecessor 방향으로 닫은 집합.
- E_max: p 자신과 p의 모든 successor를 제외한 집합.

실제 가능한 E는 E_min⊆E⊆E_max를 만족한다. D의 요구 contributor가 E_max 밖에 있거나 E_min이 D에 없는 행을 이미 완성했다면 이 p,D 조합은 불가능하므로 제외한다.

아직 삭제되지 않은 행의 contributor 중 E_min에 없는 것이 q 하나뿐이라면, 이 frame에서 q는 아직 실행되지 않았어야 한다. 따라서 E_max에서 q와 그 successor들을 제외할 수 있다. 이는 줄 지우기의 부정 조건을 사용한 정확한 상계 축소이며, 최대 board에 실제로 공존할 수 없는 blocker를 남겨 두어 증명이 실패하는 경우를 줄인다.

남은 p,D마다 어떤 ILC realization에 대해 아래를 동시에 증명한다.

1. 그 lock mask가 해당 D에서 compact한 p의 원래 셀과 정확히 같다.
2. E_min의 board에서도 floor/grounding이 성립한다.
3. E_max의 board에서도 lock까지의 전체 수직 경로가 비어 있다.

fixed D의 compaction은 occupancy의 포함관계를 보존한다. 따라서 작은 board에서 성립하는 grounding은 실제 board에서도 성립하고, 큰 board에서 비어 있는 경로는 실제 board에서도 비어 있다. E_max가 실제로 가능한 실행 집합일 필요는 없다. 더 많은 셀로도 길이 비어 있음을 보이는 보수적인 상계이기 때문이다.

수직 경로가 막히는 경우에는 **이미 관측해 보관한 하나의 positive reachability certificate**를 조회하는 대안을 연결했다. 같은 catalog owner·profile·D·정확한 target pose에서 proof mask M과 점유 b가

$$
B_{\min}\cap M=B_{\max}\cap M=b
$$

를 만족하면 구간 전체에서 같은 물리 결과가 유지된다. 초기 spawn/seed, collision, grounding, earlier first-success kick 경쟁을 포함한 기존 proof만 사용하며, 조회 자체는 새로운 BFS나 footprint 컴파일을 실행하지 않는다. 두 endpoint를 각각 조회해 성공했다는 사실만으로 중간 상태를 인정하지 않는다. 두 endpoint가 서로 다른 증명으로 성공했을 수 있기 때문이다.

같은 proof cache를 켠 기존 producer는 각 board의 point lookup에서도 BFS를 이미 피할 수 있다. 따라서 구간 증명 횟수를 primitive state 감소량으로 해석하면 안 된다. 이번 측정은 graph node, interval guard, primitive state를 분리해서 기록한다.

이 certificate를 모든 poset-available p,D에 대해 얻으면 두 포함관계가 동시에 성립한다.

$$
\operatorname{PhysicalOrders}(T)\subseteq\operatorname{LE}(P)
\quad\text{및}\quad
\operatorname{LE}(P)\subseteq\operatorname{PhysicalOrders}(T).
$$

즉 equality다. 모든 순서를 개별로 확인한 결과가 아니라 각 전이에 공통으로 적용되는 증명이다. 줄 지우기는 contributor 조건과 D별 ILC를 통해 처리한다. 하드드롭 경로 또는 이미 관측한 동일 positive footprint가 구간 전체의 전이를 보증하므로 해당 전이에서 새로운 kick 탐색은 필요하지 않다. 후자의 최초 관측 비용은 별도로 남는다.

certificate가 실패했다는 것은 해가 없다는 뜻이 아니다. 회전·kick이 필요한 실제 해, OR 형태의 받침, 배치 후 잠시 막혔다가 줄 삭제 후 다시 가능한 관계는 여전히 존재할 수 있다. 이때는 기존 exact BuildUp을 호출한다.

## 5. 증명된 언어의 직접 컴파일

`compile_poset_language`는 concrete BuildOrderGraph를 입력으로 받지 않는다. kind 배열과 증명된 predecessor mask를 받아, 같은 kind의 안전한 twin class를 만들고 class별 남은 개수로 residual을 표현한다. 각 출력 kind에서 가능한 child 언어는 기존 canonical language cache에 정확하게 intern한다. 동일 kind의 서로 다른 class가 같은 prefix를 만들 때도 child를 임의로 하나만 선택하지 않는다.

컴파일한 root를 기존 `StandardBagCoverage`에 넘기면 기존 hold·bag semantics와 원래 큐 합집합을 그대로 사용한다. 이 경로에서는 CandidateProjection의 physical-board subset table, BuildNode/BuildEdge, primitive reachability 방문 상태를 만들지 않는다. 언어 residual과 supply product는 남을 수 있으며 별도 계수한다.

일반 poset의 residual 수는 여전히 지수적일 수 있으므로 bounded budget을 둔다. 예산을 초과하면 None=Unknown으로 돌려 기존 producer를 사용한다. 미완성 언어를 exact 언어로 반환하거나 resource 예산으로 false prune하지 않는다.

## 6. 더 일반적인 조건 그래프에 대한 다음 단계

### 6.1 고정 poset 밖의 조건

“S는 특정 배치 이전 또는 줄 지우기 이후에 가능하다”는 관계는 단일 before edge로 표현할 수 없는 경우가 있다. 일반 형태는 g_p(E,D)라는 Boolean guard다. positive predecessor뿐 아니라 OR, negative predecessor, 삭제행 사건을 보존해야 한다.

따라서 exact poset은 빠른 부분집합이고, 전체 표현은 clear event를 공유하는 guarded operation graph가 되어야 한다. 고정 frame에서는 영향이 없는 operation들을 factor하고, clear event에서는 관련된 guard를 다시 specialize한다. min/max bound로 증명하지 못하는 경우에도 자동으로 edge를 추가하거나 삭제하지 않는다.

### 6.2 확률만 필요한 경우의 하한·상한 포화

정확한 성공 집합 Q가 어려워도 sound한 하한 L과 상한 U에 대해 L⊆Q⊆U를 증명할 수 있다. 같은 유한 universe에서 overflow 없는 정수 집합 크기가 |L|=|U|라면 L=Q=U다. 이 경우 미해결 도달성 분기를 더 탐색할 필요가 없다.

그러나 float 확률의 반올림 값이 같다는 사실은 이 증명이 아니다. 0-weight queue가 있는 분포에서는 exact weighted mass equality도 queue set equality를 보장하지 않는다. 잘린 universe에서는 그 materialized universe 내부에만 성립하며 전체 공간의 completeness를 새로 얻는 것이 아니다. 이번 구현에 이 saturation 경로를 추가하지는 않았다.

### 6.3 pre-tiling 공급과 APDP

이번 certificate는 완성 candidate의 concrete row contributor를 필요로 한다. 현재는 tiling 이후이되 BuildUp 이전에 만든다. tiling 이전 공통 분석으로 옮기려면 catalog의 (placement,D)별 lock·support·수직 경로 조건을 미리 컴파일하고, candidate 선택 때 contributor ID만 specialize해야 한다.

partial APDP에서 X와 E를 구분해야 하며, 아직 선택하지 않은 placement를 이미 실행한 것으로 취급하면 안 된다. 필요한 row contributor나 지원 placement의 일부가 아직 결정되지 않았다면 overapprox domain으로 유지한다. 현재 empty-field 경로가 `compile_minimum`을 사용한다는 사실도 반영해야 한다. 상세 APDP에만 연결하면 빈 필드 P7P4에는 효과가 없다.

### 6.4 반복 증명 비용을 catalog로 옮기는 구조

현재 certificate를 약 3천만 candidate마다 만들면 실패하는 증명 비용도 누적된다. 다음 최적화는 placement 사이의 거대한 adjacency를 만드는 것보다, `(placement,D)`의 support/corridor를 원래 좌표의 cell mask로 저장하는 방식이 적합하다.

$$
\operatorname{Compact}_D(\operatorname{cells}(q))\cap F\ne\varnothing
\iff
\operatorname{cells}(q)\cap\operatorname{Lift}_D(F)\ne\varnothing.
$$

candidate에서는 최대 64개 cell에 concrete operation bit를 대응하는 고정 128-byte map을 만든다. support mask는 최대 4개 cell이므로 supporter 집합은 소수의 lookup·OR로 구한다. 각 q를 매번 compact하고 전체 operation을 순회할 필요가 없다. 이미 실행한 q의 살아남은 셀은 포함하되 q의 pending ILC domain으로 필터링하지 않는다.

catalog row별 valid-D bitset과 시작 offset을 저장하고, 실제 유효한 `(p,D)`마다 support/corridor 두 u64만 저장하면 대략 `32R + 16K bytes`다. R은 catalog row 수, K는 유효 frame record 수다. byte/record cap과 기존 memory admission을 적용하고 넘으면 기존 계산으로 돌아가야 한다. 이 layout과 수치는 후속 설계이며 이번에 구현·측정한 저장 구조는 아니다.

고정 D의 contributor closure도 한 번 계산해 p별 predecessor와 OR할 수 있다. harddrop proof는 support-operation bitset과 minimum, blocker-operation bitset과 maximum의 교집합으로 처리하고, 실제 physical minimum/maximum board는 cached movement guard를 시도할 때만 만든다. 작은 D=0 gate metadata부터 적용하면 실패 candidate가 많은 상황에서 손익을 따로 확인하기 쉽다.

이 개선은 certificate가 증명하는 명제를 바꾸지 않고 같은 판정을 더 싸게 얻는 것이다. 그다음으로 OR support·clear phase를 포함하는 guarded relation의 적용률을 넓히고, source coverage 포화로 아직 남은 물리 질문을 없애는 순서가 적절하다.

### 6.5 전체 확률 포화 뒤에는 후보의 존재성만 계산한다

CountUnique이고 해별 coverage가 불필요하며, 같은 universe의 전체 성공 집합이 이미 포화되었다면 새 candidate가 성공 큐를 하나라도 갖는지로 계산 목표를 낮출 수 있다. 기하 후보와 canonical identity 수집은 계속해야 한다. minimum cover, 해별 chance evidence, 실행 제약, observation policy, CountAll, 대표 trace 등 별도 소비자가 있는 경우는 따로 제외해야 한다.

현재 `result.rs`와 `parallel_worker.rs`는 `standard_bag_coverage_complete()`를 확인하지만, 기존 `CandidateWitnessMode`는 concrete pattern index가 있는 단일 큐를 요구한다. 따라서 symbolic standard bag에서는 포화 후에도 전체 coverage product가 남는 경계가 있다. 후속 `BagMembershipOnly` 경로는 full-multiset certificate에서는 공급 개수만 확인하고, 일반 exact poset에서는 첫 성공에서 끝나는 bag·hold 존재성 product를 사용하도록 분리할 수 있다. 임의의 위상정렬 하나가 실패했다고 candidate를 버려서는 안 된다.

병렬 경로의 현재 포화는 worker-local이다. 첫 구현은 이 범위를 유지하고, worker 사이에 신호를 공유하려면 같은 universe에 대한 단조로운 complete 증거를 명시적으로 전달해야 한다. 이번 코드에는 이 목표 전환을 추가하지 않았다.

## 7. 구현 및 검증 기록

### 7.1 범위와 코드

실험 flag는 `CLEARRA_STRUCTURAL_ORDER=1`이며 test 또는 `local-search-ab` feature에서만 제공한다. 기존 `CLEARRA_PLACEMENT_DEPENDENCY=1`은 두 비교군 모두 유지한다. 일반 제품 feature의 기본 동작은 바꾸지 않는다.

code commit은 `f121ec47bb5eb2272b6ee440f0e0a259209996f8`, tree는 `db74b2c428272c74c762458ae3a1f95d5a4f1b0c`다. 동일 tree의 local build commit은 `9c33692ff680d65efd280547148a92debf1881b8`이다. release binary SHA-256은 `09a658a6f06c73a92ef0b7dd29dfd677353fbb6b19d423d807f565662ddaac07`이다.

주요 구현 파일은 다음과 같다.

- `placement_poset.rs`: necessary predecessor와 min/max sufficiency certificate, 삭제되지 않은 행의 singleton absence.
- `piece_order_language.rs`: poset 직접 언어 컴파일, twin count quotient, 별도 one-bag scalar 산술.
- `reachability_dependency_cache.rs`, `reachability.rs`: 같은 positive certificate로 전체 occupancy interval을 보증하는 읽기 전용 조회.
- `buildup.rs`: CandidateProjection 생성 전 증명된 candidate를 직접 supply product로 연결.
- `structural_order.rs`: process 시작 시 선택하는 실험 모드와 성공 bypass 계수.
- `structural_graph_probability_tests.rs`: independent physical oracle, fallback, 실제 캐시 재사용, 해별 전체 queue/probability 검증.
- `examples/placement_graph_bench.rs`: `--scalar-demo`와 동일 바이너리 structural A/B.

CountUnique·ClearToEmpty·symbolic standard bag에 한정하며 trace, finesse, spin coverage, execution constraint, observation policy, witness shortcut이 필요한 후보는 기존 경로로 보낸다. cached interval guard는 기존 관측 record가 있을 때만 가능하다. structural flag만 켠 cold cache가 추가 BFS로 record를 만드는 것은 아니다.

### 7.2 정확성 시험

관련 고유 시험 **72개 통과, 기존 2개 ignored**다. ignored는 외부 생성 asset이 필요한 기존 시험이며 통과 수에 포함하지 않았다. 전체 workspace CI나 배포 gate를 통과했다는 의미는 아니다.

| 시험 그룹 | 통과 |
| --- | ---: |
| poset 언어·산술 | 5 |
| 새 구조 certificate·공급 통합 | 5 |
| reachability 및 dependency cache, 새 interval 3개 포함 | 51 |
| 기존 language fusion | 3 |
| 기존 graph·확률 통합 | 3 |
| 기존 placement dependency 통합 | 5 |

4-operation forward DAG 64개에 distinct/repeated kind를 각각 적용해 총 128가지 그래프·label 조합의 전체 종류 단어 집합을 explicit operation oracle과 비교한다. 자유 I 15개의 경우 32,768개 concrete subset 대신 16개 residual count state와 1개 class를 사용한다. 이 수치는 언어 모델 시험이며 표준 7-bag에서 I 15개를 공급할 수 있다는 뜻이 아니다.

실제 I/O 카탈로그 4종류는 5개 kick profile에서 모든 reachable operation subset·다음 operation·전체 종류 단어를 independent BFS와 비교한다. OR support 및 roof fixture는 certificate 실패를 불가능으로 쓰지 않는 반례다. 실제 full/reverse 관측으로 만든 interval proof는 가변 cell 4개의 모든 16개 board를 별도 oracle과 비교하고, 잘못된 owner/profile/frame/pose·음성 proof를 거절한다.

대표 경로를 요구하는 Trace 출력은 첫 후보에서 graph를 유지하므로, no-trace인 CoverageSummary도 별도로 실행한다. Include의 실제 해별 coverage를 Trace 기준과 비교하고 동일 weights로 확률 report를 재구성해, 빈 report 배열끼리 비교하는 검증을 피했다.

| 실제 필드 fixture | 해 / 성공 큐 | graph nodes 전→후 | primitive states 전→후 | graph bypass |
| --- | --- | ---: | ---: | ---: |
| I 하나 | 1 / 1 of 7 | 2→0 | 0→0 | 1 |
| O 두 개, 초기 hold O | 1 / 12 of 42 | 4→0 | 0→0 | 1 |
| roof OI, hold off | 1 / 2 of 42 | 4→4 | 10→10 | 0 |
| 혼합 4미노 | 135 / 173 of 840 | 1,675→1,513 | 1,114→1,114 | 25 |

혼합 fixture는 135개 해 전부의 coverage와 확률을 비교한다. 162개 language residual, cached interval guard 1개를 사용했다. roof fixture에서 기존 질의로 10개 primitive state를 먼저 생성한 뒤에는, certificate 자체의 추가 primitive state 0개로 IO와 OI 두 순서를 정확히 얻었다. 준비 비용 10개를 숨기거나 baseline 대비 10개 절감으로 주장하지 않는다.

Scalar demo는 개별 큐 0개, 물리 field search 0회로 다음 값을 계산했다. 앞 세 가지는 ideal DP 방문도 0이며, 일반 N 구조는 irreducible ideal 상태 1개가 남았다.

| 명시적으로 공급한 poset | 정확한 분자 / 분모 | ideal states |
| --- | ---: | ---: |
| 7개 독립 | 5,040 / 5,040 | 0 |
| 7개 chain | 1 / 5,040 | 0 |
| 3개 chain + 4개 독립 | 840 / 5,040 | 0 |
| 4개 N 구조 | 5 / 840 | 1 |

### 7.3 전체 P7P4 측정

빈 10×4 field, 4줄 PC, 10개 배치/11개 공급(P7P4), 빈 hold, Jstris 180, CountUnique, full-future-oracle 조건이다. Rust `WasmCpuSearchBackend`를 native host에서 실행했다. C core·브라우저 WASM·GUI·CLI 경로의 시간으로 일반화하지 않는다. tablebase, exact legal-board, conditioned reachability 가속은 껐고, 자동 worker 9개 및 `use_all_logical_processors=true`를 두 군에서 유지했다. A는 기존 placement dependency 개선만 사용하고, B는 여기에 structural flag를 추가한다.

성공 실행 전체가 다음 evidence와 일치했다.

| 검산 대상 | 결과 |
| --- | --- |
| 전체 canonical 해 수 | 456,459 |
| 전체 해 식별자 집합 SHA-256 | `08c477a2f71455a72778f076f9942789a9997ab3b3008fc2a3d043f02458f062` |
| normalized solution digest | `cts1:2fcbc950acdffa31` |
| 성공 큐 / 전체 큐 | 4,233,600 / 4,233,600 |
| 전체 coverage 비트셋 SHA-256 | `0d7794467640c727b89ccd5f170fc4430228e0aba6fa9d7f8657b0e653ef3912` |
| 전체 packing 후보 | 29,856,840 |
| packing 집합 digest | `d6715a89054ef642` |
| 완료 / 잘림 | complete / truncation 없음 |

개별 해 확률은 Omit 모드였다. 따라서 큰 탐색은 **전체 해 식별자와 전체 coverage 합집합**을 검산했으며, 456,459개 각각의 probability/queue export까지 비교한 것은 아니다. 해별 coverage·확률의 exact 비교는 §7.2의 작은 fixture에서 수행했다. 전체 coverage 100%는 모든 개별 해의 확률이 100%라는 뜻도, 미래 큐를 보지 않는 online 정책의 성공률이 100%라는 뜻도 아니다.

실행 순서는 A1 → B1 미완료 → B1 재실행 → B2 → A2 → A3다. 기준 시간의 큰 변동을 확인한 뒤 같은 바이너리·9 workers로 A3를 추가해 재확인했다. B1 미완료는 106.565초에 manager `reason=nonzero`, `return_code=125`로 끝났으며 결과 JSON은 비어 있었다. timeout 900초나 관리자 pressure 종료 기록은 없었다. manager가 신호 종료를 125로 합쳐 기록하므로 원인을 특정할 수 없고, OOM으로 확정하지 않는다. 이 실행은 완료 시간 평균이나 정확성 통과 수에 넣지 않는다. 이후 실행은 동일 자원을 유지하고 상위 shell이 자식 종료 상태를 추가 기록하도록 했다.

| 성공 실행 | structural | 검색 시간(초) | graph nodes | primitive states | manager 관측 최대 RSS(bytes) |
| --- | --- | ---: | ---: | ---: | ---: |
| A1 | off | 201.995361 | 637,208,218 | 3,252,098,721 | 5,152,989,184 |
| B1 재실행 | on | 130.871238 | 636,901,840 | 3,251,174,465 | 5,256,728,576 |
| B2 | on | 122.345993 | 636,901,668 | 3,248,725,070 | 5,339,414,528 |
| A2 | off | 109.294918 | 637,208,218 | 3,245,001,254 | 5,308,715,008 |
| A3 | off | 119.260684 | 637,208,218 | 3,245,225,620 | 5,295,808,512 |

RSS는 Linux에서 약 2초 간격으로 관측한 프로세스 그룹 메모리의 최댓값이다. kernel high-water mark가 아니므로 짧은 spike를 놓칠 수 있다. 정상 완료한 위 실행의 manager memory pressure 이벤트는 모두 0이다.

| structural 처리량 | B1 재실행 | B2 |
| --- | ---: | ---: |
| 직접 언어 경로로 넘긴 candidate | 3,477 | 3,478 |
| 생성한 language residual | 242,904 | 242,968 |
| 같은 kind twin quotient로 줄인 operation | 4,102 | 4,106 |
| 기존 positive interval proof 사용 | 3 | 4 |

전체 후보 대비 직접 처리 비중은 약 0.0116%, graph node 감소는 약 0.0481%다. 관측 cache의 worker별 상태가 달라 직접 처리 건수도 일부 달라질 수 있다. primitive states는 같은 모드에서도 병렬 배분과 cache 상태에 영향을 받으므로 위 차이를 새 certificate가 절감한 상태 수로 단정하지 않는다.

모든 성공 실행의 단순 평균은 A=143.516987초(n=3), B=126.608616초(n=2)로 B가 11.7814% 짧다. 그러나 기준 A 자체가 201.995초, 109.295초, 119.261초로 크게 변했다. A1을 제외한 후반 기준 두 번의 평균은 114.277801초이며 B의 평균은 그보다 10.7902% 길다. 후반 비교는 변동 원인을 살펴보기 위한 사후 비교이고 A1을 폐기한 공식 speedup 추정이 아니다. 모든 실행값을 위에 남기며, p95·신뢰구간·통계적 유의성은 주장하지 않는다.

따라서 현재 자료로 새 경로의 성능 향상을 확립했다고 보고하지 않는다. 증명된 subset에서 물리 graph 생략은 확인했지만, 작은 적용률과 실패 candidate의 반복 증명 비용을 개선해야 한다. 증명 비용을 독립 stage로 계측하지 않았으므로 시간 차이의 원인을 정량적으로 분해했다는 주장도 하지 않는다. 실험 flag는 기본 off를 유지한다.

완료한 다섯 검색을 Rust `--compare`로 함께 비교한 결과는 `equivalent=true`, `differences=[]`다. manager receipt ID는 순서대로 A1 `1795134317314211000-5`, B1 재실행 `1795134785994809123-5`, B2 `1795134930618049689-5`, A2 `1795135085192045204-5`, A3 `1795135266471599063-5`다. 미완료 B1은 `1795134578652257477-5`로 별도 기록했다.

raw JSON·stderr·test log는 `_local/artifacts/graph-only-probability-20261120/`에 보관한다. 비교는 Rust runner의 `--compare`를 사용하며, 다른 바이너리·입력·CPU·기본 튜닝을 혼합하지 않는다. 이전 turn의 별도 바이너리 시간이나 사용자의 12논리 프로세서/약 4분 결과를 이번 structural speedup 계산에 넣지 않는다.

재현 명령의 핵심은 다음과 같다. 빌드에는 `--features parallel,local-search-ab`를 사용한다.

```bash
CLEARRA_PLACEMENT_DEPENDENCY=1 CLEARRA_STRUCTURAL_ORDER=1 \
  build/cargo/default/release/clearra-manage runtime run \
  --producer benchmark --profile benchmark-search --timeout 900 -- \
  build/cargo/default/release/examples/placement_graph_bench
```

A군은 `CLEARRA_STRUCTURAL_ORDER=0`으로만 바꾼다. raw 출력을 저장할 때는 위 `_local/artifacts/` 경로를 사용한다. `--small`은 작은 preflight, `--scalar-demo`는 field search 없는 산술 예시이며 full P7P4의 대체 측정값이 아니다.

## 8. 후속 작업 기준

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/placement_poset.rs`, 구현 내용: cross-kind precedence가 없는 exact certificate를 FullMultiset으로 분류하고 실제 P7P4 적용률을 먼저 계측한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage.rs`, 구현 내용: 계약이 맞는 P7P4 FullMultiset 및 포화된 upper set을 35-bit 사건으로 표현하고 기존 symbolic root와의 정확한 union을 연결한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs`, 구현 내용: 전체 coverage 포화 후 남은 소비자를 확인해 BagMembershipOnly로 전환하고 language/coverage 전체 생성 대신 exact 존재성만 판정한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs`, 구현 내용: placement·D별 immutable guard 근거를 타일링 전에 컴파일하고 candidate별 재계산을 줄인다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/placement_poset.rs`, 구현 내용: fixed-poset certificate가 실패하는 OR support와 clear phase를 exact guarded relation으로 확장하고 독립성 증명의 범위를 계측한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage.rs`, 구현 내용: 증명된 lower/upper language의 동일 universe 정수 coverage 포화로 추가 도달성 검증이 불필요한 경우를 판정한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/result.rs`, 구현 내용: 해별 scalar probability와 queue materialization의 계약을 분리하되 동일 identity의 coverage 합집합 및 symbolic arena lifetime을 보존한다.

## 참고문헌과 코드 근거

- Brightwell, G.; Winkler, P. *Counting Linear Extensions*, Order 8, 225–242 (1991). [논문](https://www.math.ucdavis.edu/~deloera/MISC/LA-BIBLIO/trunk/BrightwellGraham/brightwelllinext.pdf). 일반 문제의 계산 복잡성에 대한 근거다.
- Darwiche, A. *A Compiler for Deterministic, Decomposable Negation Normal Form*, AAAI-02. [논문](https://cdn.aaai.org/AAAI/2002/AAAI02-094.pdf). deterministic/decomposable representation과 model counting의 관계에 대한 근거이며 이번 코드가 d-DNNF compiler를 구현했다는 의미는 아니다.
- Kangas, K. et al. *Counting Linear Extensions of Sparse Posets*, IJCAI 2016, 603–608. [논문](https://ijcai.org/proceedings/2016/92). component 분해를 이용한 exact counting의 관련 연구다.
- 직접 구현 근거: `buildup.rs`, `placement_poset.rs`, `piece_order_language.rs`, `standard_bag_coverage.rs`, `structural_order.rs` 및 해당 exact differential tests.

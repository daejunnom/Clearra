# 관측 정보집합의 몫과 oracle 잔여 계산의 정확성

이 문서는 see-7의 공급·관측 계산을 줄일 수 있는 조건을 증명한다. 작은 입력의
테스트 결과를 큰 입력에 외삽하지 않는다. 아래 귀납은 임의의 유한 공급 우주와
유한 완성 행동 관계의 모든 경로를 대상으로 하며, 구현의 각 축약이 만족해야 하는
전제도 분리한다. 실제 실행 결과와 성능은 별도 실험 문서의 책임이다.

기하적 미노 관계가 올바르게 주어졌다는 전제가 있다. 이 문서는 킥·spawn·천장의
정당성을 새로 증명하지 않는다. 그 부분은
[조합론적 보드 가족 증명](combinatorial-board-family-proof-2026-11-22.md)과
[의존성 주 경로](dependency-primary-path-2026-11-22.md)의 범위다.

## 1. 상태와 제품 계약

### 1.1 공급 우주와 관측

유한한 원래 pattern ID 집합을 \(\Omega\), 각 ID의 비음수 가중치를
\(\mu(q)\)라 한다. 같은 조각 단어를 가진 서로 다른 ID도 구분한다.
확률의 분모는 원래 공급 우주의 질량이며, 축약 과정에서 ID나 질량을 버리지 않는다.

공급 단어 \(q\)의 현재 cursor를 \(c\), hold를 \(H\), 관측 폭을 \(w\)라 한다.
현재 `VisibleSeven`의 \(w\)는 7이며, 코드의 관측 창은 **현재 cursor에서 시작하는
일곱 source draw**다. active 조각 외에 별도의 일곱 preview를 추가로 센다는 뜻이 아니다.
기본적으로 \(d(c)=\min(N,c+w)\)까지의 공급이 관측된다. 이미 본 과거 공급과
행동도 정보의 일부이며, bag·terminal 계약이 읽는 과거를 임의로 잊지 않는다.

현재 관측 이력과 양립하는 원래 ID 집합을 \(I\subseteq\Omega\)라 한다.
이 집합은 사용자에게 주어진 패턴·bag 제약과 실제 공개 이력으로 결정된다.
**특정 후보나 행동에서 성공할 큐만 먼저 남긴 집합은 관측 정보집합이 아니다.**
성공 가능성으로 큐를 먼저 거른 뒤 숨은 미래가 확정됐다고 판단하면, 플레이어가
아직 모르는 정보를 정책에 제공하게 된다.

현재 구현의 observation trie는 같은 길이의 공급 단어를 요구한다. 일반적인
가변 길이 우주에 정리를 적용하려면 EOF도 관측·전이의 기호로 보존해야 한다.
불완전하거나 잘린 우주를 넘긴 경우 증명은 그 주어진 우주에 상대적인 것이며,
누락된 외부 큐까지 포함한 완전성을 주장하지 않는다.

### 1.2 물리 행동과 hold

Boolean PC의 물리 lock 행동은 `(piece kind, 현재 물리 네 셀)`로 식별한다.
공급 행동은 `Current`, `SwapHeld`, `StoreCurrent`를 포함한다. 하나의 정책 행동은
공급 선택과 실제 lock 선택을 함께 확정한다. 현재 공개 계약은 다음 순서다.

1. 현재 관측으로 공급/hold 분기와 물리 lock을 선택한다.
2. 선택한 조각을 잠그고 줄을 지운다.
3. 변경된 cursor에 따라 공급을 공개하고 다음 행동을 선택한다.

따라서 `StoreCurrentUseNext`는 hold와 lock 사이에 새로운 정책 선택을 넣지 않는다.
빈 hold 직후 preview를 공개하는 다른 게임 정책은 이 문서의 동치 범위가 아니다.
근거는 [Queue Knowledge 계약](../setup-search.md#queue-knowledge) 및
[`SupplyExecutionAutomaton::transition`](../../crates/clearra-supply/src/execution_automaton/supply_execution_automaton.rs)이다.

`ReleaseHeldAtTerminal`은 cursor를 증가시키지 않지만, 현재 evaluator에서는
**다음 language node가 accepting일 때만** 허용된다. 비종료 행동을 반복할 수 있는
무료 hold 전이가 아니다. 실제 원래 공급 길이 \(N_{mat}\)와, bag에서 결정적으로
투영한 lookahead까지 포함하는 trie 길이 \(N\)는 다를 수 있다.
이 구분과 terminal-held-release 조건은 축약 후에도 보존한다.

## 2. 완전 물리 언어와 잔여 관계

후보 \(t\)의 정확한 완성 물리 행동 언어를 \(L_t\)라 한다. prefix 관계는
구체적인 operation ID, 원래 타일링, 삭제 행 frame을 소유한다. 원래 operation을
현재 물리 행동으로 바꿀 때는 그 prefix의 삭제 행 projection을 사용한다.

실제로 수행한 공통 물리 이력 \(h\) 이후 살아 있는 후보 상태의 집합을
\(R(h)\)라 정의한다. 빈 이력에서는 모든 후보 root가 들어 있고,

\[
R(ha)=\{\operatorname{child}(s,a):s\in R(h),\ a\text{가 }s\text{에서 허용됨}\}.
\]

### 정리 1: 공통 물리 이력에 따른 잔여 집합은 정확하다

임의의 유한 이력 \(h\)에 대해 \(R(h)\)는 원래 후보 중 \(h\) 전체를 허용한
상태만을, 그리고 그런 모든 상태를 포함한다.

**증명.** 길이 0에서는 root 집합의 정의다. \(h\)에서 명제가 참이라고 하자.
원래 후보가 \(ha\)를 허용하면 \(h\)를 허용한 상태에서 \(a\) 전이가 있으므로
갱신식에 들어간다. 반대로 갱신식의 모든 원소는 \(h\)를 허용한 실제 후보 상태의
\(a\) 자식이므로 \(ha\)를 허용한다. 두 방향이 성립하므로 귀납이 끝난다. □

이 정리는 후보별 eager DAG를 유지하거나 `(candidate, executed prefix)`를
필요할 때만 전개하는 것에 의존하지 않는다. 후자가 같은 갱신식을 구현하고
완전한 후보 집합을 유지하면 저장 방식이 달라도 완성 언어는 같다.

- **전방 보존:** 모든 원래 완성 실행이 잔여 관계의 완성 실행으로 간다.
- **역방향 보존:** 잔여 관계의 모든 완성 실행은 처음부터 같은 물리 이력을 허용한
  원래 후보 실행으로 돌아간다.

한 방향의 simulation만으로는 두 성질을 모두 얻지 못한다. 허용 행동을 더 많이
남기는 upper relation은 거부 증거에는 쓸 수 있어도 새로운 양성 witness를
자동으로 인증하지 않는다. upper liveness로 제거된 행동도 국소적인 물리 미도달과
구분해야 한다.

### 종류 단어로의 투영과 그 한계

물리 행동에서 종류만 남기는 사상을 \(\pi\)라 하자. 정확한 종류 언어는
\(K(h)=\pi(\bigcup_t h^{-1}L_t)\)다. 고정된 공급 단어와 정확한 hold automaton에 대해

\[
\exists\text{ 호환되는 물리 완성 실행}
\iff
\exists\text{ 호환되는 }K(h)\text{의 종류 단어}
\]

가 성립한다. 전방은 투영이고, 역방향은 종류 단어가 정확한 투영 언어에 속한다는
정의에 따른 물리 witness의 존재다. 종류 언어의 합집합·결정화도 **완전 단어
언어를 정확히 보존하는 경우**에 한한다.

그러나 미래가 숨겨져 있을 때는 큐별로 물리 witness가 존재한다는 사실만으로
하나의 관측 정책이 그 witness들을 실행할 수 있다고 결론 내릴 수 없다.
\(\forall q\,\exists\text{ 실행}\)과
\(\exists\text{ 관측 가능한 정책}\,\forall q\)의 선택 순서가 다르기 때문이다.
따라서 숨은 단계에서는 같은 종류의 다른 lock을 합치지 않는다.

## 3. 정책 값과 oracle 동치의 완전한 조건

상태 \(z\)는 잔여 물리 언어, 공급/hold 상태, 관측 정보집합 및 terminal 계약을
포함한다. 완성 시 해당 실행은 종료하며 accepting 상태 이후를 다시 실행하는
다른 계약을 섞지 않는다. 정책은 같은 \(z\)에 있는 모든 큐에서 같은 행동 \(a\)를 선택한다.
그 뒤 새 공개 \(o\)별로 정보집합이 분할된다.

완성 시 성공 ID 집합의 질량과 ID 개수를 값으로 보고, 기존 구현처럼 질량을
먼저, 개수를 다음으로 비교할 수 있다. 수학적 재귀는

\[
V(z)=\max_a\sum_o V(z[a,o])
\]

다. accepting node에서는 terminal 공급 조건을 만족하는 원래 ID의 질량과 개수를
반환한다. 각 lock이 잔여 완성 깊이를 감소시키므로 terminal-held-release를
포함해도 유한 깊이에 대한 귀납이 가능하다.

### 정리 2: oracle는 관측 정책의 상계다

각 \(q\in I\)에 대해 oracle는 전체 공급을 알고 성공 실행 하나를 선택할 수 있다.
oracle가 성공 가능한 ID 집합을 \(W(z)\)라 하면, 모든 관측 정책의 성공 집합은
\(W(z)\)의 부분집합이다. 따라서 비음수 가중치에서

\[
V(z)\le\bigl(\mu(W(z)),\ |W(z)|\bigr).
\]

**증명.** 관측 정책의 특정 큐에서의 성공 실행은 oracle도 선택할 수 있는 실제
실행이다. ID별 포함관계와 비음수 합에서 바로 따른다. □

### 정리 3: 한 관계에서 oracle와 같을 필요충분조건

관측 정책이 oracle의 모든 성공 ID를 보존할 필요충분조건은 다음 재귀 조건이다.

- accepting node 또는 \(W(z)=\varnothing\)에서는 성립한다.
- 그 외에는 하나의 현재 행동 \(a\)가 존재하여, 모든 \(q\in W(z)\)를 그 행동의
  후속 oracle-success 집합 안에 남기고, 각 다음 관측 블록에서도 같은 조건이 성립한다.

**증명.** 그런 행동이 존재하면 관측별 후속 정책을 결합해 모든 \(W(z)\)를 덮는다.
반대로 모든 oracle 성공 ID를 덮는 정책의 첫 행동은 하나로 고정되어 있으므로
어느 성공 ID도 버릴 수 없고, 각 후속 관측에서도 같은 성질이 필요하다.
잔여 lock 깊이에 대해 양방향으로 귀납한다. □

이 조건은 숨은 큐가 여럿이어도 공통으로 성공하는 정책이 있으면 동치임을 보여 준다.
따라서 “숨은 suffix가 유일함”은 일반적으로 충분조건이며 필요조건은 아니다.
필요충분조건 자체를 평가하려고 큰 정책 탐색을 다시 수행하면 최적화의 목적을
잃을 수 있으므로, 구현은 아래의 값싼 구조적 충분조건을 사용한다.

## 4. 결정적 미래의 oracle closure

### 정리 4: 앞으로 읽을 공급과 terminal 데이터가 확정되면 oracle로 접을 수 있다

현재 \(I\)의 모든 큐가 남은 행동들이 읽을 수 있는 공급 값, 공급 종료 위치,
terminal 판정 데이터에서 같다고 하자. 그러면 같은 잔여 관계에서 모든 큐의
실행 가능성이 같고, 성공할 수 있다면 하나의 동일한 행동 실행을 모두에게 선택할
수 있다. 따라서 관측 정책 값은 oracle 값과 같다.

**증명.** 동일한 초기 잔여 상태와 동일한 공급/terminal 데이터에서 한 행동의
활성화, 공급 갱신, 다음 상태가 같다. 행동 길이 귀납으로 모든 완성 실행의 허용
여부가 같다. 성공 실행이 있으면 그 하나를 선택하는 정책은 아직 공개되지 않은
값에 의존하지 않는다. 정리 2의 상계를 달성한다. □

이때 정리 1의 정확한 잔여 관계를 종류 언어로 바꿔 기존 oracle 공급 연산기로
계산할 수 있다. 같은 증명은 숨은 단계가 1회였는지 100회였는지와 무관하다.
한 representative 큐로 계산하더라도 결과의 질량·개수·bitset에는 \(I\)의 **모든
원래 ID**를 그대로 대응시켜야 한다. 동일 단어의 중복 ID도 예외가 아니다.

### 4.1 단순 관측 창 certificate

유효한 cursor \(0\le c\le N\)에서 \(N-c\le w\)이면 남은 유한 공급 전체가 이미
보인다. `QueueObservationPolicy::fully_observes_source(N,c)`는 이 조건을 공통 공급
API로 제공한다. \(N\)은 lock 수가 아니라 projected lookahead를 포함한 source 길이다.
뺄셈은 cursor 범위 검사 뒤 수행하므로 큰 `usize`에서도 덧셈 overflow에 의존하지 않는다.

이 helper는 보이는 창에 관한 충분조건이다. 보이지 않는 값을 bag 규칙으로 추론할
수 있는지, terminal 조건이 다른 외부 데이터를 읽는지는 대신 판단하지 않는다.

### 4.2 실제 정보집합의 유일한 suffix certificate

균일 길이의 정확한 observation trie에서, 현재 노드부터 terminal leaf까지 각
비종료 노드의 유효한 조각 자식이 정확히 하나라면 남은 suffix가 유일하다.
역방향 위상 순서에서 다음 식으로 판단할 수 있다.

\[
\operatorname{forced}(v)=
\begin{cases}
\mathrm{true} & v\text{가 실제 terminal leaf},\\
\operatorname{forced}(u) & v\text{의 유일한 자식이 }u,\\
\mathrm{false} & \text{그 밖의 경우}.
\end{cases}
\]

**증명.** leaf에서 남은 단어는 빈 단어 하나다. 유일한 자식 아래 단어가 하나면
그 앞에 고정된 조각을 붙인 단어도 하나다. 서로 다른 조각 자식이 두 개 이상이면
서로 다른 첫 조각을 가진 suffix가 존재한다. 높이에 대한 귀납이다. □

여러 원래 ID가 한 leaf에 놓여도 조각 미래는 유일하다. 빈 정보집합은 별도의
값 0으로 처리하며 존재하지 않는 representative witness를 만들지 않는다.
가변 길이를 지원하려면 terminal과 continuation의 선택도 분기로 취급해야 한다.

이 계산은 기존 future-class의 역방향 위상 순서에서 함께 수행할 수 있다.
이미 소유한 trie/정확한 class를 재사용하므로 원래 큐를 다시 열거할 필요가 없다.
certificate는 language를 oracle 잔여 표현으로 바꾸는 권한이며, 실제 observation
node를 미래 leaf로 바꾸거나 cursor를 진행시킬 권한이 아니다. 원래 reveal과
terminal 집계는 그대로 유지한다.

### 4.3 bag으로 더 일찍 확정되는 경우

정확한 bag 제약 아래 현재 bag의 남은 종류가 하나이면 그다음 조각은 확정된다.
이때 필요한 것은 bag 경계와 실제 잔여 집합이며, 단순히 preview 안에 서로 다른
여섯 종류가 있다는 조건은 bag 경계를 넘을 수 있어 충분하지 않다. 마지막 조각을
확정해도 다음 새 bag의 순서까지 확정되는 것은 아니다.

일반 패턴에서 여덟 번째 조각이 확정됐더라도 아홉 번째 이후가 여러 가지라면 전체
미래의 결정성을 얻지 못한다. 확정된 부분이 아래의 모든 미래 읽기 범위를 덮는지,
또는 trie의 전체 남은 suffix가 유일한지를 확인해야 한다.

### 4.4 결정적인 공급에서는 종류 union DAG도 전개할 필요가 없다

공통 물리 이력 뒤 정확한 residual을 이루는 종류 언어를
\(K_1,\ldots,K_m\)이라 하자. 각 \(K_j\)는 여러 완성 종류 단어를 허용할 수
있다. 여기서 개별 membership이라는 말은 언어가 단어 하나라는 뜻이 아니라,
\(\bigcup_jK_j\)를 새 DAG로 전개하지 않고 이미 있는 각 root에 공급 적합성을
질의한다는 뜻이다.

**정리 4A.** 현재 원래 정보집합 \(I\)의 모든 큐가 남은 각 언어에서 읽을 수 있는
공급 값·EOF·terminal 데이터에서 같으면, 각 root의 성공 집합 \(W_j\)는
\(\varnothing\) 또는 \(I\)다. 따라서

\[
W_{\cup_jK_j}=\bigcup_j W_j
=\begin{cases}
I & \exists j:\ W_j=I,\\
\varnothing & \text{그 밖의 경우}.
\end{cases}
\]

**증명.** 같은 공급·hold 상태와 같은 미래 입력에서 한 고정 root의 활성 전이와
종료 판정은 같다. 정리 4의 행동 길이 귀납에 의해 한 큐에 성공 실행이 있으면
그 실행이 모든 \(I\)에서 성공하고, 없으면 어느 큐에도 없다. 또한 하나의 완성
종류 단어가 언어 합집합에 속할 필요충분조건은 그 단어가 적어도 한 \(K_j\)에
속한다는 것이다. 따라서 합집합에서 성공하는 실행은 원래 root 하나의 완성
실행으로 돌아가며, 역방향 포함도 즉시 성립한다. 성공 root 하나를 현재 정보만으로
선택할 수 있으므로 관측 정책에서도 같은 결과를 얻는다. □

그러므로 root들을 순서대로 검사하다가 \(I\) 전체를 성공시키는 root를 발견하면
나머지 root와 union의 자식들을 만들지 않고 종료해도 된다. 모두 실패하면 정확히
실패다. 이 최적화는 같은 physical history를 거친 모든 root를 유지한 뒤, 관련
미래가 확정된 경계에서만 쓴다. 숨은 단계의 worker별 정책 값을 합치는 연산이 아니다.

반례는 §5.3의 서로 다른 첫 O 배치다. 관련 미래가 확정되기 전에는 각 root가 서로
다른 큐에 성공할 수 있어 \(W_j\)가 \(I\)의 진부분집합이다. 이때 단일 root의
정책값만으로 union의 정책을 대신하면 잘못 제거하거나 잘못 허용할 수 있다.
성공 큐만 남긴 뒤 결정성을 판정하는 것도 이 전제를 충족하지 못한다.

모든 root의 union 목록은 완전해야 하고, 숫자 ID는 같은 kind arena와 고정 깊이에
묶여 있어야 한다. 취소·자원 거부를 해당 root의 membership 실패로 바꾸면 안 된다.
전체 성공의 판정은 가중치가 상계에 가깝다는 비교 대신 원래 ID 개수로 한다.
인증된 공통 양수 가중치의 count 실행에서 \(0<|W_j|<|I|\)가 나오면 증명 전제가 깨진 것이므로
이를 전체 실패로 뭉개지 않고 명시적인 불일치로 거부한다.

## 5. 일곱 조각의 의미: draw와 lock을 구분해야 한다

### 정리 5: 현재 hold 규칙에서 공급 소비량의 상계

시작 hold 점유 여부를 \(h_0\in\{0,1\}\), \(k\)번 lock 뒤 점유 여부를
\(h_k\)라 하자. terminal-held-release에서는 실제 hold를 비운 것으로 센다.
현재 순차 공급 전이에는 다음 보존식이 성립한다.

\[
c_k-c_0=k+h_k-h_0.
\]

**증명.** `Current`와 `SwapHeld`는 한 lock과 한 draw를 수행하며 점유 여부가 같다.
`StoreCurrent`는 빈 hold를 채우면서 한 lock과 두 draw를 수행한다.
terminal-held-release는 점유 hold를 비우면서 한 lock과 0 draw를 수행한다.
각 단계에서 양변의 증가량이 같으므로 전이 횟수에 대한 귀납으로 성립한다. □

따라서 앞으로 최대 \(r>0\) lock을 수행할 때 읽을 수 있는 source draw 수는

\[
D_{max}=r+\mathbf 1[\text{hold 허용, 현재 hold 비어 있음}]
\]

이하다. \(r=0\)이면 공급 전이의 추가 draw는 0이다. hold를 비웠다가 비종료 상태에서
다시 채울 수 있는 다른 규칙에는 이 상계를 적용하지 않는다.

terminal 판정이 추가 hidden supply를 읽지 않고, 위 범위 안의 모든 공급 값과
EOF가 현재 정보로 정해졌다면 정리 4를 적용할 수 있다. 이것은 미래 reveal을
없애도 결과에 필요한 모든 입력을 이미 알고 있다는 충분조건이다.

### 5.1 고정 horizon의 관련 draw만 관측됐다는 certificate

정확한 완성 언어가 모든 accepting 실행의 총 lock 수 \(n\)을 보증하고,
현재 language 깊이가 \(k\)라 하자. \(r=n-k\)가 남은 lock 수다. 현재 원래
observation node의 깊이를 \(d\), 유효 source 길이를 \(N\)으로 두면

\[
d\;\ge\;\min(N,c+D_{max})
\tag{K}
\]

는 앞으로 lock 선택에 영향을 줄 수 있는 source 위치가 이미 모두 보인다는
충분조건이다. 단어 위치는 0부터 시작하므로 깊이 \(d\)는 위치 \(d-1\)까지
공개됐다는 뜻이다. 모든 단어의 EOF는 균일 길이 \(N\)으로 이미 고정돼 있다.
\(r=0\)일 때 \(D_{max}=0\)으로 처리한다.

이 certificate는 실제 원래 observation node에 적용한다. 후보가 성공할 큐만
추린 node나 더 긴 suffix를 가진 것으로 추정한 node를 대신 사용하지 않는다.
정상 전이 후 필요한 마지막 draw의 상계도 증가하지 않는다. `Current`와
`SwapHeld`는 \(c\)를 1 늘리고 \(r\)을 1 줄이며, 빈 hold의 `StoreCurrent`는
\(c\)를 2 늘리는 대신 \(r\)을 1 줄이고 추가 draw 항을 1에서 0으로 바꾼다.
따라서 한 번 (K)로 인증한 후 새로 공개되는 미사용 suffix가 이후의 물리 선택에
필요해지는 일은 없다.

구현은 이 추가 certificate를 **terminal supply target이 없고, language가 정확한
`fixed_horizon`을 제공할 때만** 사용한다. `fully_revealed_node`는 관련 미래가
확정된 잔여 관계의 종류 투영을 뜻하며 실제 observation node, cursor, hold 또는
reveal 순서를 바꾸지 않는다. 값 계산 `solve`와 선택 ID 복원 `collect_selected`에
동일한 정규화를 적용해야 한다. 원래 전체 source를 보았다는 더 강한 조건이나
실제 suffix가 유일하다는 조건은 (K)와 별개의 충분조건이다.

### 5.2 종료 lookahead와 마지막 hold 방출의 소스 전제

`SearchProblem`의 공급 compiler는 `projects_unplaced_lookahead`를 무조건 켜지
않는다. 정확한 geometry lock 수가 \(n\), 처음 hold 점유가 \(h_0\)일 때 유한
materialized source가 \(N_{mat}=n-h_0\)에서 끝나고, hold 때문에 자동 공급 창이
한 조각 더 필요할 때만 켠다. 현재 standard-bag 제품은
`StandardBagCoverage::supports`가 초기 cursor \(c_0=0\)도 확인한다.

따라서 정상 \(n\)-lock 종료에서는 정리 5에 의해

\[
c_n=N_{mat}+h_n.
\]

이는 원래 `accepts_pattern_path`의 projected 종료 조건인
`cursor == Nmat && hold.is_none()` 또는
`cursor == Nmat + 1 && hold.is_some()`와 같다. target 없는 accepting node에서
별도 숨은 공급 값을 읽지 않는 것이 이 제품 범위에서 정당화된다. 임의 길이
accepting 언어나 compiler 계약과 무관하게 만든 공급 길이에 이 결론을 적용하지 않는다.

실제 materialized EOF에서 `ReleaseHeldAtTerminal`을 사용하는 경로는 마지막 held
조각을 잠그고 종료한다. 이 추상 전이는 0 draw이며 남은 lock 수를 1 줄이므로
(K)의 상계를 늘리지 않는다. 이 경우 sentinel hold는 논리적으로 빈 hold를 뜻한다.

표준 bag의 여섯 조각 뒤에서 마지막 조각을 추론할 수 있다면 trie는 그 결정적인
조각을 붙여 \(N=N_{mat}+1\)로 만든다. 원래 oracle의
`can_release_held_at_terminal`은 이때 shortcut 방출을 거부하고 실제 inferred
current와 `SwapHeld`를 쓰게 한다. observation evaluator는 shortcut도 허용하지만,
**추가 terminal 목표나 spawn/finesse 조건이 없는 Boolean lock 언어**에서는 같은
held 조각을 같은 footprint에 잠그는 정상 `SwapHeld`가 반드시 존재한다. 두 경로는
최종 cursor/hold 표기는 다르지만 같은 성공 ID를 허용한다. 이런 마지막 경로 중복은
Boolean 존재 판정의 차이가 아니며, CountAll이나 최종 hold 상태의 동치 증거도 아니다.

미사용 current의 spawn 가능성이나 종료 hold·bag remainder를 검사하는 제품이라면
이 두 경로의 동치를 다시 증명해야 한다. 자동 의존성 PC 경로는 현재 finesse와
그 입력 이력을 거부하며, (K) shortcut은 `terminal_supply_target.is_none()`을
요구한다. 이 제한을 “lookahead는 모든 제품에서 무관하다”로 확대하지 않는다.

| 현재 상태 | 일반적인 안전 충분조건 |
| --- | --- |
| 전체 유효 source가 7 draw 이하 | 초기 cursor 0에서도 전부 관측됨 |
| hold 금지 또는 이미 채워진 hold | 남은 \(r\le7\) lock이 읽을 최대 draw가 보임 |
| 빈 hold 사용 가능 | 남은 \(r\le6\) lock이면 최대 \(r+1\le7\) draw가 보임 |
| 빈 hold, 7 lock, 8 draw 이상 가능 | 일반적인 oracle 동치를 보장하지 못함 |

표의 lock 조건은 terminal 공급 제약이나 더 먼 숨은 값의 읽기가 없다는 전제를 가진다.
`SetupTerminalSupplyTarget`처럼 종료 시 bag remainder를 읽는 제품은 그 데이터의
결정성까지 별도로 보존해야 한다.

### 5.3 7-lock 반례: 빈 hold는 숨은 여덟 번째 조각을 사용할 수 있다

각각 질량 \(1/2\)인 다음 두 큐를 둔다.

- `Q_T = I O T S Z J L T`
- `Q_S = I O T S Z J L S`

처음 일곱 draw는 같고, 여덟 번째만 다르다. 다음 두 physical trace만 있는 합성
완성 언어를 생각하자. `O_a`, `O_b`는 다른 네 셀에 잠그는 행동이다.

- `O_a, T, S, Z, J, L, T`
- `O_b, T, S, Z, J, L, S`

각 trace는 **7 lock**이다. 최초 I의 lock은 허용되지 않아 I를 빈 hold에 넣고
O의 위치까지 선택해야 한다. 그 시점에는 마지막 T/S가 아직 보이지 않는다.
나중의 hold 선택으로 반대 trace를 성공시킬 수도 없다. `Q_T`에는 S가 하나뿐이고
`Q_S`에는 T가 하나뿐이어서 각 trace에 필요한 두 번째 종류가 없다.

따라서 see-7의 최댓값은 \(1/2\), oracle의 값은 1이다. held I는 미사용으로 남으므로
이 반례는 terminal-held-release나 추가 lookahead에 의존하지 않는다. 두 큐는 P7P1
형식에 들어가는 유효한 word지만, 이 합성 행동 관계가 특정 실제 타일링의 전체
가능 관계와 같다고 주장하지 않는다. 이 반례는 lock 수만으로 모든 관계의 동치를
증명할 수 없음을 보인다.

그러므로 “차이는 최소 여덟 개부터 생긴다”는 표현은 **관측되지 않은 여덟 번째
source draw가 처음 필요하다**는 뜻으로 써야 한다. 빈 hold를 허용하는 lock 개수의
하한으로 쓰면 틀리다.

## 6. P7P4에서 숨은 물리 단계의 길이

P7P4의 유효 공급 길이는 11, 관측 폭은 7이다. 따라서 cursor가 4 이상이면 남은
공급 전체가 보인다. 최초 hold가 비어 있으면 한 행동이 2 draw를 소비할 수도 있어
일부 경로는 더 일찍 이 경계에 도달한다.

### 정리 6: depth 4 이후 비종료 상태는 모두 완전 관측이다

시작 cursor가 0인 P7P4에서 모든 **비종료 상태로 가는** lock은 cursor를 적어도
1 증가시킨다. 0 draw인 terminal-held-release는 accepting 자식으로만 간다.
따라서 4회의 lock 이후 남아 있는 비종료 상태는 \(c\ge4\), 즉 \(c+7\ge11\)이다. □

이에 따라 depth 4까지는 정확한 물리 행동 commitment를 보존하고, 이후에는 정확한
잔여 종류 언어와 기존 oracle 공급 계산을 재사용할 수 있다. 실제 분기는 depth
상수보다 `cursor`와 source-domain certificate를 기준으로 결정하는 편이 일반적이다.
일찍 끝난 accepting node는 원래 terminal 계약으로 평가한다.

일반적인 유한 \(N,w,c_0\)에서도 같은 전제 아래
\(\max(0,N-w-c_0)\)회의 비종료 lock 뒤에는 전부 보인다. 이 상계는 특정 타일링,
operation 수, 경로의 개수에 의존하지 않는다.

완전한 표준 bag P7P4 우주에서 첫 bag의 순서가 이미 알려졌다고 하자. cursor가
0, 1, 2, 3, 4일 때 두 번째 bag에서 알려진 조각 수는 각각 0, 1, 2, 3, 4다.
그 관측과 양립하는 남은 4-draw suffix의 수는 다음과 같다.

| cursor | 알려진 두 번째 bag draw | 가능한 남은 suffix 수 |
| ---: | ---: | ---: |
| 0 | 0 | \(7P4=840\) |
| 1 | 1 | \(6P3=120\) |
| 2 | 2 | \(5P2=20\) |
| 3 | 3 | \(4P1=4\) |
| 4 | 4 | 1 |

그러므로 이 **완전 우주**에서는 bag 규칙만으로 cursor 3의 마지막 숨은 draw를
유일하게 정할 수 없다. 더 제한된 실제 입력 패턴에서는 가능하며, 그 경우는
원래 정보집합의 unary-suffix certificate로 판정한다.

## 7. 관측 상태의 재사용: 가중 미래의 몫

미래 suffix의 종류 집합만 같다고 정책 값을 재사용해서는 안 된다. 각 suffix의
확률 질량과 원래 ID 개수, 현재 보이는 공급, hold, EOF/terminal 계약도 필요하다.

두 상태 사이에 다음 대응이 있다고 하자.

1. accepting 판정과 terminal 공급 결과가 대응한다.
2. 현재 선택 가능한 물리/공급 행동이 대응하며, 숨은 단계에서는 실제 물리 행동을
   임의의 kind 선택으로 바꾸지 않는다.
3. 행동 뒤 reveal이 만드는 서로 겹치지 않는 관측 블록이 대응한다.
4. 대응한 leaf/블록은 같은 비음수 질량과 같은 원래 ID 개수를 갖는다.
5. 대응한 후속 상태도 같은 관계를 만족한다.

### 정리 7: 위 대응은 최적 정책 값의 동치다

**증명.** terminal 값이 같다. 더 작은 잔여 깊이에서 값이 같다고 가정하면,
각 현재 행동의 후속 관측별 합이 같고 행동들의 최댓값도 같다. 그 행동에 대응하는
실제 행동과 후속 정책을 택하면 양방향 정책 실행도 만들 수 있다. 유한 잔여 깊이에
대한 귀납으로 끝난다. □

현재 `ObservationFutureSignature`의 조각별 자식 class, subtree 질량 및 개수는
이 가중 미래 tree를 bottom-up으로 비교하는 한 구현이다. 해시값만 같다는 이유로
상태를 합치지 않고 정확한 signature를 비교해야 한다. 같은 suffix를 갖더라도
terminal 공급이 읽는 consumed-bag history가 다르면 그것도 key에 남겨야 한다.

cursor를 key에서 제거하려면 적어도 materialized EOF와 projected EOF까지의 거리,
bag phase 및 terminal predicate가 동치임을 추가로 증명해야 한다. 단순히 현재
일곱 조각과 hold가 같다는 조건은 충분하지 않다. 재사용 memo의 language/source
namespace와 terminal 계약도 동일하거나 정확히 대응해야 한다.

가중치가 비례하는 서로 다른 블록 사이에서 조건부 값을 정규화하여 재사용하는
확장도 가능하지만, 동일한 분포·정규화 인자·원래 개수 및 수치 계약을 별도로
증명해야 한다. 현재 정확한 signature 재사용을 그 주장으로 확대하지 않는다.

### 성공 큐와 representative witness의 구분

이 정리는 최적 질량, 원래 ID 개수, 대응하는 정책의 존재를 보존한다. 선택한 정책을
실제 observation trie로 다시 실행하면 해당 우주의 원래 ID들을 복원할 수 있다.
다른 ID namespace의 bitset을 memo에 저장해 그대로 재사용해서는 안 된다.

동률 최적 정책은 서로 다른 성공 ID 집합을 가질 수 있다. canonical action ordering을
별도로 보존하지 않는 한 기존 representative bitset/replay까지 같다고 주장하지 않는다.
반면 한 관측에서 모든 원래 ID를 성공시키는 경우에는 그 관측의 성공 bitset 자체가
\(I\)로 정해진다. 어느 동률 witness를 골라도 그 완전 coverage는 같다.

## 8. 모든 ID를 덮는 행동에서의 조기 종료

### 정리 8: 정확한 pattern count가 정보집합 크기에 닿으면 더 개선할 수 없다

현재 관측의 원래 ID 집합을 \(I\), 어떤 현재 행동과 그 후속 정책이 성공시키는
집합을 \(C\subseteq I\)라 하자. 정확한 count가 \(|C|=|I|\)이면 \(C=I\)다.
따라서 비음수 가중치에서 그 행동은 정확한 상계
\((\mu(I),|I|)\)를 달성하며 나머지 현재 행동을 평가하지 않아도 된다.

**증명.** 유한 집합의 포함관계와 같은 크기에서 동일성이 따른다. 다른 행동의
성공 집합도 \(I\)의 부분집합이므로 질량과 개수를 더 크게 만들 수 없다. □

필요한 구현 불변량은 다음과 같다.

- count는 language path 수가 아니라 원래 pattern ID의 수다.
- 각 후속 reveal 블록은 서로 겹치지 않고 현재 정보집합을 분할한다.
- terminal 집계는 각 원래 ID를 한 번만 센다.
- 더한 count는 항상 부모 subtree count 이하이며, overflow를 포화 연산으로 숨겨
  거짓 full-cover certificate를 만들지 않는다.

이것은 정책 행동의 조기 종료다. 전체 타일링 후보를 세는 과정이나 CountAll의
build variant 열거까지 중단할 권한이 아니다.

### 8.1 공통 양수 가중치에서는 정확한 count를 목적값으로 쓴다

균등 공급의 실제 가중치는 pattern마다 1이 아니다.
[`WeightedPatternSet::uniform(N)`](../../crates/clearra-coverage/src/pattern/weighted_pattern_set.rs)은
각 원래 ID에 저장된 `f64` 값 \(w=\operatorname{fl}(1/N)\)를 부여한다.
여러 pattern에 weight 1을 주면 확률 질량 합 계약을 위반하므로 실행 자격을 그 값에
한정하거나 그런 입력으로 균등 우주의 회귀를 대신해서는 안 된다.

**정리 8A.** 모든 실제 입력 ID의 가중치가 하나의 양수 \(w>0\)와 같으면,
성공 ID 집합 \(C\)의 수학적 질량은 \(\mu(C)=w|C|\)다. 따라서 같은 물리·공급·관측
계약에서 최대 확률 정책은 최대 성공 count 정책과 정확히 같다.

**증명.** terminal에서는 성공한 ID의 질량이 count의 \(w\)배다. 서로소 reveal
블록의 합과 정책 행동의 최댓값에서 양수 배율은 다음과 같이 보존된다.

\[
w\sum_o n_o=\sum_o wn_o,
\qquad
w\max_a n_a=\max_a wn_a.
\]

잔여 행동 깊이에 대한 귀납으로 모든 정책 상태의 최적 질량은 최적 count의
\(w\)배다. 질량 동률도 정확히 count 동률과 같으므로 개수를 두 번째 기준으로
사용하는 기존 수학적 목적과 충돌하지 않는다. □

따라서 내부 DP는 `score`를 성공 ID 개수로 두고, 서로소 reveal에서는 더하며
행동 사이에서는 최댓값을 취한다. `score`와 외부 probability mass를 혼용하지 않는다.
실제 count는 `u32` 안에 있으므로 \(2^{32}-1<2^{53}\)이고, 이 정수 score는 `f64`에
담아도 terminal·합·비교가 정확하다. 출력 경계에서만
`covered_weight = accepted_count * w`를 계산한다. 관측 블록마다 질량으로 바꾼 뒤
다시 더하지 않는다.

`ObservationTrie::compile`은 실제 각 입력의 `weight > 0`과 `to_bits()` 일치를 모두
확인하여 이 권한을 만든다. 한 번 불일치한 뒤 다음 값으로 uniform 판정을 되살리지
않으며, 빈 입력이나 0 가중치에는 이 권한을 부여하지 않는다. 이름이 uniform이거나
가중치가 epsilon 이내로 가깝다는 조건으로 대체하지 않는다.

`UniformWithTerminalRemainder`는 마지막 실제 가중치가 다를 수 있다. 그 경우는
일반 weighted 경로에 남아야 한다. 현재 제품 admission의
[`complete_standard_bag_ordinals`](../../crates/clearra-supply/src/pattern_universe/materialized_pattern_universe.rs)는
실제 compact sequence와 `WeightedPatternSet::uniform(N)`의 동등성까지 확인한다.
따라서 자동 P7P4 제품의 균등성은 profile 이름이나 표본에 대한 추정이 아니다.

이 count 목적값 변환은 Boolean 성공 ID를 평가하는 한 가변 깊이나 추가 terminal
목표에도 성립한다. 다만 **full-cover 조기 종료**의 실행 자격에는 다음을 더 요구한다.

- language가 `fixed_horizon`을 제공하여 모든 accepting 실행의 총 lock 깊이가 같다.
- 각 reveal의 원래 ID가 서로소이고, 합친 count가 정확한 subtree count 이하이다.

그러면 full-cover에서 성공 bitset은 \(I\), 최소·최대 accepted depth는 고정 깊이다.
대표 행동이 동률 중 어느 것인지는 별도 계약이다. 기본 language는 fixed-horizon
권한을 제공하지 않으며, 가변 깊이 소비자는 조기 종료 때문에 accepted-depth
metadata가 바뀌지 않도록 기본적으로 이 cutoff를 사용하지 않는다.

### 8.2 정확한 정책 목적값과 출력 반올림을 구분한다

정리 8A에서 \(w\)는 입력에 **실제로 저장된 동일한 양수 값**이다. 의도한 균등
분포를 정확한 유리수 \(1/N\)로 보더라도, 모든 ID의 가중치가 같은 양수라는 성질
덕분에 최적 정책과 최적 count는 같다. 내부 DP의 정책 선택에는 반복 부동소수점
덧셈이나 epsilon 비교가 필요하지 않다.

외부 `covered_weight`의 `count * w`는 한 번의 `f64` 곱셈이며, 일반적으로 정확한
유리수 `count / N` 자체는 아니다. 또한 과거 코드가 \(w\)를 반복해서 더한 값이나
다른 reveal 괄호 구조로 더한 값과 마지막 비트가 다를 수 있다. 이 변경은 정확한
성공 개수와 수학적 최적 확률을 보존하지만 과거의 누적 반올림·동률 대표 선택까지
비트 단위로 보존한다고 주장하지 않는다. 확률의 분모와 원래 weight owner는
유지하며, 부분 정보집합의 크기로 \(w\)를 다시 정규화하지 않는다.

일반 비균등 입력은 기존 weighted recurrence와 합산 순서를 유지한다. 정리 4A와
정리 8의 집합 명제 자체는 비음수 가중치에서도 성립하지만, 새 count score나
개별 root shortcut을 그 입력에 자동 적용하지 않는다. 비균등 float 정책까지
산술 재현성을 바꾸려면 별도의 정확 산술 또는 합산 계약이 필요하다.

특히 이 기존 비균등 경로가 모든 실수 가중치에서 수학적 argmax까지 정확하다는
주장은 하지 않는다. 예를 들어 두 성공 집합의 가중치가 각각
\((1/4,2^{-56})\), \((1/4,2^{-55})\)이면 수학적 합은 후자가 크지만, 두 `f64` 합은
모두 \(1/4\)로 반올림될 수 있다. 성공 개수도 같으면 과거 tie 순서가 실수 질량이
작은 집합을 고를 수 있다. 이 산술 한계는 새 공통 가중치 count 경로의 반례가
아니며, 일반 weighted 경로의 기존 수치 계약을 구분해야 하는 이유다.

### 8.3 개별 root membership 실행의 수치 계약

정리 4A는 임의의 비음수 수학적 가중치에서 성립한다. 이번 실행 shortcut은
**비트가 동일한 공통 양수 가중치의 count score**, 정확한 `fixed_horizon`,
`terminal_supply_target` 없음, 원래 정보집합의 관련 미래 certificate를 함께
요구한다. 이때 root 성공값은 정확히 \((|I|,|I|)\), 실패값은 \((0,0)\)다.
여기서 첫 좌표는 probability mass가 아니라 내부 score다.

성공 root를 찾으면 나머지를 생략해도 내부 최적 count, 성공 ID 전체, 고정된
accepted depth가 같다. 외부 질량은 §8.2의 한 번의 변환으로 복원한다. root 선택
순서가 다른 representative 물리 실행을 만들 수 있다는 점은 별도로 유지한다.

`collect_selected`도 값 계산과 같은 원래 정보집합에서 전체 성공 root를 선택하여
원래 ID를 복원한다. 한 root에서만 성공한 ID를 전체 성공으로 늘리거나 여러 root의
부분 성공 bitset을 임의로 합치지 않는다. 실제 가중치가 서로 다르면 기존 종류
union 경로를 사용한다.

## 9. CountUnique와 CountAll의 경계

여기서 투영하는 계산은 원래 큐별 Boolean 성공 여부와 그 가중 확률이다.
여러 physical trace가 같은 종류 단어로 투영될 수 있으므로 \(\pi\)는 보통 단사가
아니다. 언어 합집합이 중복을 제거하는 것은 존재 여부에는 맞지만, 모든 실행이나
build variant의 중복도까지 보존하지는 않는다.

현재 자동 의존성 제품 경로는 Boolean CountUnique PC에 제한된다.
`result.rs`는 CountAll의 `build_variant_count_exact`를 별도로 표시한다.
CountAll로 확장하려면 제품이 세는 대상의 동일성, trace/candidate 중복도, 가중
semiring 또는 그에 해당하는 집계 표현을 보존해야 한다. 원래 큐 확률 계산이
빨라졌다는 이유만으로 CountAll 출력을 같은 축약으로 인증하지 않는다.

점수·스핀·B2B·finesse·입력 이력도 같은 footprint가 같은 상태를 뜻하지 않을 수
있어 별도 제품 계약이다. 이 문서는 그러한 목적의 state/cost 보존을 대신하지 않는다.

## 10. 병렬 producer와 전역 정책

### 10.1 worker의 관계와 종류 arena를 함께 이동한다

worker가 담당한 후보 언어의 합집합을 \(L_i\)라 하자. 병렬 생산의 목표는
\(\bigcup_i L_i\)를 하나의 전역 정책 평가기에 공급하는 것이다.
worker별 최적 정책의 값이나 선택 bitset을 합치는 것이 아니다.

§5의 두 7-lock trace를 서로 다른 worker에 넣으면 각 worker의 최댓값은
\(1/2\)이고 두 성공 bitset의 합집합은 전체 큐다. 하지만 두 첫 O의 물리 위치가
다르므로 전역 정책의 최댓값도 \(1/2\)다. 반대로 첫 물리 행동이 같고 이후의
공개에 따라 후보를 선택할 수 있는 경우에는 전역 언어가 각 worker 단독보다
좋은 정책을 허용할 수 있다. 따라서 worker 값의 합뿐 아니라 단순 최댓값도
일반적인 언어 병합을 대신하지 못한다.

현재 native 전달은 `ObservationProducerShard`에 정확한 physical/factored 관계와
그 suffix ID들을 소유한 `PieceOrderLanguageCache`를 함께 담는다. root 숫자만
다른 worker로 복사하지 않는다. coordinator가 아래의 정확한 DAG remap을 수행한
뒤 모든 후보를 한 물리 residual owner에 합쳐 정리 1의 갱신식을 적용한다.

### 정리 9: 종류 DAG의 완전 remap은 모든 잔여 언어를 보존한다

원래 node의 자식이 부모보다 먼저 정의되는 유한 DAG를 생각하자. terminal부터
순서대로, source node \(v\)의 각 edge `(kind,u)`를 `(kind,f(u))`로 바꾸고 같은
accepting/depth와 정확한 edge 목록을 destination에서 intern하여 \(f(v)\)를 만든다.
그러면 모든 source node에서 \(L(v)=L(f(v))\)다.

**증명.** terminal은 같은 빈 단어를 허용한다. 더 작은 자식 언어가 보존된다고
가정하면 각 edge의 `kind`를 앞에 붙인 언어와 그 합집합도 같다. 두 방향의 완전
단어 포함관계가 모두 성립한다. 위상 순서에 대한 귀납으로 끝난다. □

`import_node_map`은 실제 child 순서와 depth를 검사하고 전체 node의 대응표를
반환한다. `merge_factored`는 **모든 보유 prefix suffix root**를 그 대응표로 바꿔야
한다. 최초 root만 바꾸면 depth 4 같은 후속 oracle closure에서 다른 worker의
숫자 ID를 소비하게 된다. 해시가 같다는 이유만으로 언어를 같다고 취급하지 않는다.
같은 완성 operation proof를 재사용할 때도 geometry와 row frame의 대응은 유지한다.

### 10.2 생산 분할의 완전성과 성공 후보 필터

병렬화는 같은 초기 보드·catalog·규칙·공급·목표 계약에 결박된 요청 안에서만
관계를 병합한다. 모든 원래 geometry branch가 정확히 한 번 완료됐다는 소유 증거가
필요하다. native `merge_results`의 canonical branch index 완전성 검사는 이 분할을
확인한다. 취소·자원 거부·미완료 branch가 있으면 완전한 정책 결과로 승격하지 않는다.

원래 우주의 어느 큐에서도 완성되지 않는 후보를 버리는 것은 안전하다. 그런 후보의
완전 언어는 어떤 관측 정책의 성공에도 기여할 수 없기 때문이다. 다만 살아 있는
후보는 **하나의 성공 witness만이 아니라 완전한 physical residual 관계**를 전달한다.
전역 coverage가 이미 채워졌다는 이유로 다른 후보의 가능한 정책 행동을 생략해서도
안 된다. 현재 `CandidateWitnessMode::for_candidate`는 관측 정책에서 단일 witness
shortcut을 사용하지 않는다.

동일 후보가 중복 생산되어도 Boolean 언어 합집합은 멱등이지만, 그것이 branch 누락을
허용하거나 CountAll의 중복도 검사를 대체하지는 않는다. branch completeness와
종류 DAG/physical 언어의 정확성은 각각 확인한다.

paired owner는 worker workspace에서 이동한 뒤 scratch snapshot을 얻는다.
이동한 payload는 `observation_producer_bytes`로 따로 세며, 같은 kind arena와 physical
관계를 scratch와 반환 payload 양쪽에 중복 계산하지 않는다. 과거 worker exit 합계는
coordinator의 현재 arena와 동시에 살아 있는 할당의 peak라는 뜻이 아니다.

### 10.3 최초 관측별 정책 병렬화는 별도로 가능하다

최초 공개 후 서로 겹치지 않는 관측 블록들이 \(\Omega=\bigsqcup_j I_j\)를 이루면,
첫 행동 전에 어느 \(I_j\)인지 알려져 있으므로

\[
V(\Omega)=\sum_j V(I_j)
\]

다. 각 블록의 최적 정책을 실제 최초 관측에 따라 고르는 정책으로 결합하면 된다.
이 분할은 후보별 worker 정책을 합치는 잘못된 방법과 다르다. 각 블록의 worker는
전역의 **모든 후보 언어**를 보아야 하며, 최초 관측 root들을 누락·중복 없이 나눠야 한다.

이 방식은 immutable trie와 완성 producer들을 공유하고 각 worker가 자신의 lazy
arena/memo를 가지는 추가 최적화 후보다. 일반 가중치에서 기존 부동소수점 합산 순서까지
유지하려면 최초 root별 값을 원래 순서로 합쳐야 한다. 이 수학적 분해가 가능하다는
사실만으로 해당 병렬 소비자 구현이나 성능 측정을 완료했다고 표시하지 않는다.

## 11. 소스에서 정리로 이어지는 검토 항목

테스트 개수는 아래 전제를 대신하지 않는다. 각 최적화는 자신이 재사용하는 결과의
소유자와 다음 대응을 유지해야 한다.

| 소스 경계 | 연결되는 수학 전제 |
| --- | --- |
| `QueueObservationPolicy::fully_observes_source` | cursor 범위와 실제 source 길이로 정리 4의 단순 관측 certificate를 만든다. lock 수를 길이로 대입하지 않는다. |
| `ObservationTrie::compile` | 원래 우주 및 실제 정보집합, 균일 길이, ID·질량 보존. 실제 모든 가중치의 양수·비트 일치로 count score 권한을 만들며, 후보 성공 큐로 조건을 추가하지 않는다. |
| `ObservationTrie::compile_future_classes` | 실제 조각 자식의 정확한 signature와 역위상 순서. unary suffix를 같은 소유 자료에서 증명한다. 공통 양수 가중치에서는 정리 8A의 count score를 signature에 사용한다. |
| 관측 evaluator의 `solve` 및 `collect_selected` | 전체 source 관측, 원래 정보집합의 unary suffix, 또는 terminal target 없는 fixed horizon의 관련 draw certificate에서 잔여 language만 oracle 표현으로 바꾼다. 실제 observation/cursor/hold는 유지하고 양쪽 경로에 같은 변환을 적용한다. |
| 물리 language의 완전 공개 adapter | 정리 1의 현재 residual을 정확한 종류 언어로 투영한다. 미래가 숨겨진 상태에서는 이 권한을 사용하지 않는다. |
| `revealed_alternatives` / 개별 kind root adapter | 정리 4A의 완전한 root 목록과 같은 arena·깊이를 보존한다. 인증된 미래·공통 양수 weight의 정확한 count score·fixed horizon·target 없음에서만 union 대신 개별 membership을 사용하며, 직접 kind node는 다시 alternatives로 자기 자신을 반환하지 않는다. |
| `FactoredPhysicalActionLanguageView::prepare_edges` | 현재 공급에서 실행 가능한 모든 종류의 physical derivative를 준비한다. 나중에 다른 종류를 추가해도 기존 edge index와 자식은 바꾸지 않는다. |
| `SupplyExecutionAutomaton::transition` | Current/Swap/Store 소비량과 hold 점유 보존식. 무관한 다른 hold 규칙을 섞지 않는다. |
| `best_transition` / `transition_value` | 같은 관측에서 행동을 먼저 선택하고 reveal 블록의 합을 계산한다. full-cover 종료는 원래 ID count의 정확한 상계 및 공통 양수 weight·fixed horizon 권한에 의존한다. 내부 score에 raw probability mass를 섞지 않는다. |
| 공급 compiler / `accepts_pattern_path` / terminal-held-release | §5.2의 fixed-depth·초기 cursor·lookahead 전제로 target 없는 종료가 보존된다. inferred-current SwapHeld와 0-draw 방출의 최종 hold 상태까지 같다고 주장하지 않는다. |
| `acceptance_value` / `SetupTerminalSupplyTarget` | 추가 terminal 목표는 원래 target 판정을 유지한다. 관련 draw만 보는 (K) certificate는 이 목표가 있을 때 사용하지 않는다. |
| memo class 및 재사용 domain | 종류·질량·개수뿐 아니라 필요한 visible/hold/cursor/terminal history와 owner namespace를 보존한다. |
| `ParallelWorkerResult` / `ParallelSearchOutcome` | worker의 physical proof와 kind arena를 함께 이동하며 독립 worker 정책값을 합치지 않는다. |
| `PieceOrderLanguageCache::import_node_map` / `merge_factored` | 정확한 child·depth·accepting node의 위상 remap과 모든 보유 suffix root의 대응. geometry/operation 연결 보존. |
| native `merge_results` / `complete_internal` | 원래 branch 생산의 완전성과 같은 요청 계약을 확인하고 하나의 전역 physical 언어를 최종 정책으로 평가한다. |
| 결과 조립 | quotient 계산값을 원래 ID로 복원한다. uniform count score는 마지막에만 공통 weight를 곱해 외부 질량으로 바꾸며, 거부·취소를 완료된 실패 확률로 바꾸지 않는다. |

주요 소스는
[`queue_observation_policy.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy.rs),
[`queue_observation_physical_actions.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_physical_actions.rs),
[`queue_observation_factored_actions.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_factored_actions.rs),
[`symbolic_prefix_orders.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_prefix_orders.rs),
[`공급 policy helper`](../../crates/clearra-supply/src/queue/queue_observation_policy.rs),
[`SupplyExecutionAutomaton`](../../crates/clearra-supply/src/execution_automaton/supply_execution_automaton.rs)다.

회귀는 7/8 draw 경계, 7-lock empty-hold 반례, 10/11 draw의 숨은 선택,
동일 정보집합의 unary suffix, terminal 조건, 가중 미래 재사용 및 full-cover의
산술 경계를 검산해야 한다. 그러한 유한 검산의 통과가 위의 모든 입력·경로에 대한
귀납을 대체하지 않으며, 이 문서는 아직 실행하지 않은 검산을 통과했다고 표시하지 않는다.

## 12. 최종 제품 보장과 남는 경계

위 정리들은 정확한 유한 physical 관계와 명시된 관측·공급 계약을 전제로 한다.
입력 크기를 늘린 몇 번의 실행으로 일반화한 결론이 아니다. 제품 코드가 이 전제를
제공하는 자동 경로와, 일반 정리만으로 인증되지 않는 기존 경로를 구분한다.

현재 [`pc_placement_dependency_eligible`](../../crates/clearra-problem/src/search_problem.rs)의
자동 PC 범위는 폭 10, 실제 geometry 높이 1–6, 고정 required cells를 덮는 1–11개
operation, `ClearToEmpty`, `CountUnique`와 Boolean `All`/`Unique` 목적, CPU/Auto 요청이다.
완전한 canonical standard-bag 공급, 초기 cursor 0과 fresh bag 상태를 요구한다.
추가 점수·실행 이력·finesse·colored replay 제한·특정 setup/build 제품은 이 경로의
권한을 받지 않는다. 실제 kick/shape/all-sky 계약은
[킥 매개변수 증명](parametric-kick-equivalence-2026-11-22.md)의 별도 authority가 확인한다.
이 범위는 `exact_pieces` 옵션이 없을 때도 catalog의 required-cell 수로 같은
총 operation 수를 증명한다.

| 출력 또는 주장 | 이번 경로의 보장 |
| --- | --- |
| 숨은 단계의 실행 관계 | 같은 종류라도 다른 physical lock은 구분하며, 같은 실제 물리 이력을 가진 후보 continuation만 합친다. |
| 최적 see-7 확률 | 현재 atomic hold+lock→reveal 계약, 정확한 관계, 실제 공통 양수 가중치에서 성공 count로 정확한 수학적 최적 정책을 계산한다. |
| Oracle와의 동치 | 전체 미래 또는 모든 관련 draw/terminal 입력이 알려진 상태에서 성립한다. 숨은 미래가 남은 일반 see-7 전체를 oracle와 같다고 주장하지 않는다. |
| 선택 ID bitset | 선택된 하나의 최적 관측 정책이 실제 성공시키는 원래 ID 집합이다. 모든 동률 최적 정책의 성공 집합을 다시 합친 값은 아니다. |
| 대표 실행·동률 ID | 동률 최적 정책이나 arena 생성 순서에 따라 달라질 수 있다. 모든 ID를 덮는 블록에서는 그 블록의 bitset이 고정된다. |
| 외부 probability mass | 정확한 count에 실제 공통 weight를 한 번 곱한다. 과거 반복합이나 정확한 유리수와 마지막 반올림 비트까지 같다고 약속하지 않는다. |
| 0·혼합·비균등 가중치 | count certificate를 받지 않고 기존 weighted recurrence를 유지한다. 언어 보존과 일반 실수 산술의 수학적 최적성은 서로 다른 주장이다. |
| 거부·취소·자원 한계 | 완료된 실패 확률로 바꾸지 않는다. 지원 영역이나 표현·작업량 한계를 넘으면 완전한 결과를 반환했다고 표시할 수 없다. |

현재 hold 입력은
[`HoldSlot`](../../crates/clearra-supply/src/hold/hold_slot.rs)의 `Empty` 또는 알려진
`Occupied(PieceKind)`다. 알려지지 않은 초기 hold 종류를 임의로 하나로 고정한
모델이 아니다. 정상 `StoreCurrent`와 `SwapHeld`가 보관하는 current는 그 행동 전에
이미 관측된 조각이므로 같은 정보집합에서 hold의 종류도 같다. horizon 뒤의 미사용
공급은 terminal 목표나 실제 spawn 조건이
읽지 않는다는 §5.2의 범위에서만 무관하며, hold 직후 별도 공개를 허용하는 게임
정책도 현재 atomic 계약의 증명에 포함하지 않는다.

[`evaluate_observation_language`](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs)는
factored physical owner가 없을 때 기존 `RootedPieceLanguageUnion` 경로를 여전히
갖는다. 그 fallback이 숨은 단계의 모든 physical commitment를 보존한다고 이
문서가 인증하지는 않는다. 따라서 이번 결과를 앱 전체의 모든 legacy see-7 소비자,
CountAll, score/spin/finesse 제품, 또는 BuildUp의 전면 제거 완료로 확대하지 않는다.

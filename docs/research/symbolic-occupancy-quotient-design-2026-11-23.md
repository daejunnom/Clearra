# Symbolic movement의 점유 보드 표현과 정확한 역필터 설계

작성일: 2026-11-23. **Exact nodewise cell image와 eager `(z,p)` 역필터는 명시적
연구 경로로 구현하고 유한 fixture로 검증했다.** 구현과 완료한 로컬 검증은
[`symbolic-cell-occupancy-2026-11-23.md`](symbolic-cell-occupancy-2026-11-23.md)에 기록했다.
이 문서의 safe-superset domain, lazy `(z,p,M)` 역필터와 §7의 점유·미노 수량을 함께
쓰는 geometry quotient는 아직 구현하지 않은 대안이다.
모든 설계 제안을 완료했다거나 Full4·Full6가 더 빠르고 메모리 사용이 제한된다는
성능 주장은 하지 않는다.

## 1. 현재 구현과 바꾸려는 경계

[`symbolic_family.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family.rs)는
source/Hold 전이로 얻은 각 상태의 원래 joint relation을 유지한다. 같은 종류의 미노가
허용되는 selected-row 집합을 먼저 합치고, legality와 geometry 검사를 한 번 수행한
뒤 원래 상태별 joint relation에 exact semijoin을 적용한다. 기존 `solve_symbolic_family`
경로의 BDD 변수는 catalog row ID다. 따라서 같은 칸들을 차지하는 서로 다른 미노
분해도 그 movement 변수 공간에는 별도 선택으로 나타난다.

여기서 설계하고 새 opt-in API에 구현한 경계는 **movement 계산의 변수만 원래 칸의
점유 여부로 바꾸는 것**이다. 기존 4L authority에서는 최대 40칸, 6L에서는 최대 60칸이다.
selected-row identity, 원래 queue, Hold 상태와 geometry family를 합쳐 버리지 않는다.
반환할 해법·source 관계는 계속 원래 ZDD에서 보존한다.

현재 symbolic 계약은 Boolean 실행과 CountUnique이며, 점수·스핀 보존·입력 비용 등
추가 경로 의존 objective는 허용하지 않는다. 그런 계약을 확장할 때 이 문서의 상태가
충분한지는 다시 검토해야 한다. 미노 종류와 ordered kick/spawn authority도 movement의
고정 입력으로 유지한다.

## 2. 안전한 물리 상태: 원래 점유 U에서 삭제 frame D와 보드 B로

초기 점유 mask를 I, catalog row r의 원래 칸 mask를 C_r, 실행한 row 집합을 S라고
하자. 원래 좌표에서의 누적 점유는 다음과 같다.

\[
U(S) = I \cup \bigcup_{r\in S} C_r.
\]

폭 W의 원래 가로줄 y가 모두 차면 D_y(U)=1이다. 삭제되지 않은 원래 줄 y의 물리
높이는 그 아래에서 살아남은 줄 수다.

\[
D_y(U)=\bigwedge_{x=0}^{W-1}U_{yW+x},\qquad
h_y(U)=\sum_{j<y}(1-D_j(U)).
\]

삭제되지 않은 줄의 칸을 h_y로 옮기면 물리 보드 B(U)가 된다. 따라서 U가 같으면
D와 B가 모두 같다. 같은 종류·같은 movement authority에서 이동 가능 pose와 grounded
lock은 B에 의해 결정된다. 특정 catalog row의 ILC realization을 고르는 데에는 D가
추가로 필요하다. 새 cell 경로는 `C_r ∩ U = ∅`도 검사한다. 이미 선택한 row의 모든
칸은 U에 남아 있으므로 이 조건이 해당 row의 재선택을 막는다. 원래 row identity와
source 상관관계는 계속 원래 ZDD에서 보존한다.

이는 현재 [`SymbolicOccupancy`와 `SymbolicBoard`](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_family.rs)가
계산하는 의미와 같다. 기존 `compile` 경로는 각 칸에 대한 selected-row 변수의 OR를
만든다. 새 `compile_cells` 경로는 그 칸 자체를 primitive Boolean 변수로 삼고, 같은
줄 삭제·압축 연산을 사용한다. 현재 authority는 최대 6줄이고 초기부터 완성된 줄은
거부한다는 계약도 유지한다.

### B만으로 catalog row legality를 합치면 안 되는 실제 모양

다음은 같은 rank에서 물리 B는 같지만 D와 다음 원래 footprint가 다른 4×3 예다.
bit 0은 바닥 왼쪽이며, 각 16진수 한 자리가 아래에서 위로 한 줄을 표현한다.
초기 보드는 I=`0x113`, 채워야 할 칸은 `0xeec`다.

| 항목 | 경로 A | 경로 B |
| --- | --- | --- |
| 첫 미노의 원래 footprint | `0x06c` | `0x0e4` |
| 첫 placement 뒤 U | `0x17f` | `0x1f7` |
| 완성된 원래 줄 D | `0b001` | `0b010` |
| 삭제 뒤 물리 B | `0x017` | `0x017` |
| 다음 공통 물리 lock | `0x0e8` | `0x0e8` |
| 그 lock의 원래 footprint | `0xe80` | `0xe08` |

첫 footprint들은 각각 네 칸의 zigzag와 T 모양이며 초기 보드에서 수직으로 내려와
바닥에 놓을 수 있다. 공통 물리 lock도 동일한 굽은 미노의 수직 배치다. 두 완성 cover는
각각 `0x06c | 0xe80 = 0xeec`, `0x0e4 | 0xe08 = 0xeec`로 required cells를 채운다.

`0xe80`은 D=`0b001`에서 해당 lock으로 내려오지만 D=`0b010`에서는 pending
footprint가 삭제된 줄을 차지한다. `0xe08`은 반대다. catalog의 realization 변환은
pending footprint가 삭제 frame의 줄과 교차하면 이를 거부한다. 그러므로 B=`0x017`을
공유해 movement를 재사용할 수 있어도, 어느 원래 row가 합법인지는 D를 통해 다시
판정해야 한다. 이 예는 처음 수동 cell-mask로 검산했고, 이후
[`symbolic_original_cells_preserve_distinct_deleted_frames_on_the_same_physical_board`](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_family_tests.rs)
fixture로 옮겨 선택한 family suite에서 통과했다. U 또는 (D,B)를 보존하면 이 반례의
정보 손실을 피한다.

## 3. 물리 image 전체를 먼저 만들 필요가 있는가

piece p가 허용된 원래 joint relation을 R_p라 하면 정확한 물리 domain은 다음이다.

\[
X_p=\{U(S):\exists q,\ (S,q)\in R_p\}.
\]

현재 구현은 X_p를 원래 ZDD node별 exact image 연산으로 cell-BDD에 만든다. 그 image
구축 자체가 새로운 병목이 될 수 있다. 변수 수가 764에서 40으로 줄었다는 사실만으로
BDD 크기가 줄어든다고 보장할 수 없다. variable order와 허용 assignment 사이의
관계가 중요하다.

아직 구현하지 않은 대안은 전체 image 대신 X_p를 포함하는 명시적 domain X̂_p에서
movement predicate를 계산하고, 반환 직전에 원래 R_p를 정확히 필터링하는 것이다.
예를 들어 rank k에서 다음 조건은 기존 nonoverlapping-row 계약 아래 안전한 상위
집합을 준다.

\[
I\subseteq U\subseteq I\cup R,\qquad |U\setminus I|=4k.
\]

여기서 R은 required cells다. 4k 조건에는 모든 row가 네 칸이고 서로 겹치지 않으며
초기 칸과도 겹치지 않는다는 전제가 필요하다. 이 전제 없이 cardinality 조건을
적용하면 실제 해를 제거할 수 있다. 현재 Down(F) 및 catalog 검증이 그 전제를
제공한다. 더 일반적인 piece 집합에는 같은 숫자 4를 그대로 사용하면 안 된다.

현재 ZDD에서 모든 선택이 점유하는 must-cell mask와 어떤 선택이든 점유할 수 있는
may-cell mask를 구해 상위 집합을 좁힐 수도 있다. 이 두 mask는 cell별 독립성을
선언하는 정확한 image가 아니다. 실제 R_p의 상위 집합이라는 사실만 사용하고,
최종 역필터를 생략하지 않는다.

이 방법은 image 비용을 피하는 대신 실제로 존재하지 않는 보드에서도 movement를
계산할 수 있다. 상위 집합이 너무 크면 propagation BDD가 다시 폭증할 수 있다.
정확한 image, 안전한 상위 집합, 또는 필요한 cofactor만 만드는 혼합 방식 중 어느
것이 유리한지는 별도 계측 문제다. **아래 fused filter만 완성했다고 전체 quotient
알고리즘의 비용 문제가 해결되는 것은 아니다.**

## 4. 원래 joint ZDD와 cell predicate의 fused filter

목표 연산은 완성 보드 목록이나 selected-row 변수의 전체 pullback BDD를 만들지 않고
다음을 반환하는 것이다.

\[
\operatorname{Filter}_U(R,P)
=\{(S,q)\in R:P(U(S))=1\}.
\]

원래 joint ZDD에서 row 변수는 `[0,row_count)`, 원래 queue 변수는 그 뒤에 있다.
cell-BDD의 `[0,area)` 변수는 별개의 의미를 가진다. 기존 `FamilyBridge::filter`에
cell-BDD를 그대로 넘기면 숫자가 같은 row ID로 해석되므로 잘못된 결과가 된다.
cell predicate owner와 catalog 변환 계약이 구분된 별도 bridge가 필요하다.

### 지연 평가에서 필요한 문맥 — 미구현 대안

아래 지연 평가 방식의 traversal 상태는 `(z, p, M)`을 보존한다.

- z: 아직 처리하지 않은 원래 joint ZDD node.
- p: 아직 평가하지 않은 cell-BDD predicate node.
- M: 지금까지 선택한 row와 초기 보드가 확정한 true cell 중, p의 남은 평가에 필요한
  pending true-cell mask.

앞 선택을 p에 대입하지 않은 채 z와 p만으로 memo하면 안 된다. 서로 다른 앞부분의
row 선택이 같은 z에 도달하더라도, 아직 평가하지 않은 cell c를 먼저 점유했는지에
따라 p=`cell(c)`의 결과가 달라진다. 아래의 eager cofactor 대안은 p 자체를 제한해
그 문맥을 담으므로 M을 별도 key에서 제거할 수 있다. 그 대안도 새 residual predicate의
표현 비용을 부담한다.

### suffix의 점유 가능성

Remain(z)는 z 아래에서 아직 선택할 수 있는 모든 row의 cell mask를 합친 상위
지원 집합이다. source-only suffix와 UNIT의 Remain은 0이다. 현재 ZDD의 row 지원을
정확히 구해도 되고, catalog row ID 순서의 suffix union처럼 더 큰 안전한 상한을
사용해도 된다. 현재 eager 구현은 ZDD node별 정확한 union을 owner/epoch cache에
재사용한다. 더 큰 상한을 사용하는 변형은 pruning을 줄일 수 있지만, 가능한 cell을
false로 잘못 확정하지 않는다. 원본 geometry DAG 전체에 새 dense 표를 만드는 것은
필요 조건이 아니다.

### 전이와 pending true의 소거

1. z가 EMPTY면 즉시 EMPTY다. p가 TRUE이면 원래 z를 그대로 반환하며, FALSE이면
   EMPTY다. TRUE 반환은 원래 source suffix의 상관을 그대로 보존한다.
2. p의 앞 cell c가 M에 있으면 high cofactor로 간다. c가 M에도 없고 Remain(z)에도
   없으면 low cofactor로 간다. 아직 지원 가능성이 있으면 그 cell을 임의로 결정하지
   않는다.
3. predicate가 넘어간 cell과 그 앞의 cell은 M에서 제거할 수 있다. BDD에서 생략된
   변수는 자유 변수이므로 predicate 값에 영향을 주지 않는다. 미래에 남은 true cell은
   반드시 보존한다.
4. 다음 ZDD row가 r이면 low branch는 r을 고르지 않고, high branch는 M에 C_r을
   OR한다. 두 결과를 원래 row r의 ZDD node로 재구성한다.
5. row 변수가 끝나 source-only suffix 또는 UNIT에 이르면 Remain=0이므로, 남은
   predicate를 M의 true/나머지 false로 끝까지 평가한다. 참이면 원래 source suffix를
   반환하고 거짓이면 EMPTY를 반환한다.

Zero-suppression에 따라 ZDD에서 생략된 row 변수는 false다. 하지만 **row r의 low
branch는 r이 차지할 수 있는 cell들을 모두 false로 만드는 연산이 아니다.** 다른
suffix row가 같은 cell을 차지할 수 있다. false 확정에 Remain이 필요한 이유다.
또한 cell-P가 도중에 TRUE가 됐다고 원래 source들의 독립적 합집합을 다시 곱하지
않는다. 원래 z를 반환해야 한다.

이 전이는 cell 점유의 OR 의미에 대한 필터다. 입력에 겹친 row가 있더라도 OR 평가
자체는 정의되지만, 그것을 유효한 geometry나 rank cardinality의 근거로 승인하지는
않는다. GeometryCompatibilityFilter와 source automaton의 계약을 별도로 유지한다.

### exactness와 소유권

각 root-to-node 경로에서 M은 선택한 앞 row들이 남은 cell predicate에 주는 정보를
보존한다. high는 정확히 그 row의 칸을 추가하고, low는 row 선택만 버린다. suffix에서
더 이상 만들 수 없는 칸만 false로 정하므로 조기 판단이 실제 선택을 제거하지 않는다.
끝에서는 U가 완전히 결정되며 원래 queue suffix만 남긴다. 이 불변식으로 위의
`Filter_U` 집합 의미를 유지한다.

이 지연 평가의 memo는 `(z,p,M)`과 그 owner/epoch에 묶여야 한다. 두 arena의 compaction·rewind
뒤 숫자 ID를 재사용하거나 다른 catalog·초기 보드·cell 순서와 섞으면 안 된다.
현재 Boolean complemented edge는 정상화된 `node` API로 읽어야 한다. 기존 geometry
filter처럼 오류·취소·자원 거절은 EMPTY와 구별해 반환해야 한다.

### 대안: 즉시 cofactor를 적용하는 `(z, p)` 역필터

**이 방식은 `CellFamilyBridge::filter`에 구현했다.** 선택한 row의 칸을 cell-BDD 전체에
즉시 true로 대입하면, 제한된 residual predicate p가 앞 선택의 영향을 보존한다.
이 경우 memo key는 `(z,p)`로 충분하다. 같은 숫자의 원본 predicate를 그대로 유지하는
지연 평가와 달리, p는 선택이 진행될 때마다 바뀔 수 있다.

cell 변수 전체를 V, suffix z가 나중에 만들 수 있는 cell의 안전한 상한을 T(z)라고
하자. T는 앞서 정의한 Remain과 같은 의미이며, source-only suffix와 UNIT에서는 0이다.
실제 suffix family의 cell union을 포함해야 하지만 정확한 union일 필요는 없다.
`p[A←1]`은 A의 모든 변수를 **완전한 Boolean cofactor로 true에 고정**한 함수다.
현재 BDD의 맨 앞 node만 따라가거나 변수를 존재 양화하는 연산이 아니다. A에 속하는
변수가 더 깊은 branch에 있더라도 모두 대입해 residual에서 제거해야 한다.

초기 root를 z₀라 할 때 다음과 같이 시작하면 강한 불변식 `Vars(p) ⊆ T(z)`를 유지할
수 있다.

\[
p_0=P[I\leftarrow1]
       [V\setminus(I\cup T(z_0))\leftarrow0].
\]

초기 true 대입은 필수다. root의 바깥 cell을 false로 대입하는 둘째 항은 판단을 앞당기고
위 불변식을 단순하게 만든다. 둘째 항을 미루는 변형도 가능하지만, 그때는 아직 남은
바깥 변수를 source 경계에서 반드시 false로 평가해야 한다. 여기서는 둘째 항까지
적용하는 형태를 기준으로 설명한다.

row node z의 변수 r, low child l, high child h에 대한 두 residual은 다음이다.

\[
\begin{aligned}
p_l &= p[T(z)\setminus T(l)\leftarrow0],\\
p_h &= p[C_r\leftarrow1]
          [T(z)\setminus(C_r\cup T(h))\leftarrow0].
\end{aligned}
\]

high의 true mask C_r과 false mask `T(z)\(C_r∪T(h))`는 서로소다. 따라서 두
**완전한** cofactor는 교환하며, 위의 true-first 순서는 설명과 구현상의 선택이다.
초기 식도 true/false mask가 서로소이므로 순서 자체가 정확성의 필수 조건은 아니다.
반면 C_r을 제외하지 않은 넓은 false mask `T(z)\T(h)`를 먼저 적용하면 틀릴 수 있다.
예를 들어 P=`cell(c)`, C_r=`{c}`, T(h)=0에서 그 넓은 mask를 먼저 false로 대입하면
P가 FALSE가 되어 뒤의 true 대입으로 복구되지 않는다. 위의 정확한 false mask는
이 경우 빈 집합이므로 어느 대입 순서에서도 올바르게 TRUE를 얻는다.

각 child를 해당 residual로 필터링한 뒤 원래 row r의 ZDD node로 재구성한다. EMPTY는
EMPTY이고, residual이 FALSE이면 EMPTY, TRUE이면 원래 z를 반환한다. source-only
suffix 또는 UNIT에서는 남은 변수를 모두 false로 대입한 결과로 원래 suffix의 보존
여부를 정한다. 강한 불변식과 T=0이 유지되면 이때 residual은 이미 상수다.

이 전이의 근거는 다음 불변식이다. 지금까지 선택한 앞 row들의 칸을 A라 하고, z의
임의의 suffix 선택을 S라 하면 다음 등식이 유지된다.

\[
P\!\left(I\cup A\cup\bigcup_{r\in S}C_r\right)
   =p\!\left(\bigcup_{r\in S}C_r\right).
\]

초기 true/false 대입은 실제 suffix에서 이미 확정된 값만 고정하므로 등식이 성립한다.
low branch의 suffix는 T(l) 밖의 칸을 만들 수 없으므로, T(z)에서 T(l)을 뺀 칸을
false로 고정해도 그 suffix에서 값이 달라지지 않는다. high branch는 먼저 C_r을
실제로 점유하므로 true 대입이 정확하다. 그 뒤 C_r과 T(h) 어느 쪽에서도 제공하지
않는 칸만 false로 고정한다. 두 branch 모두 residual의 남은 변수는 child의 T 안에
있고 등식도 유지된다. 끝에서는 suffix가 더할 칸이 없으므로 residual의 값이 정확히
P(U)이며, 원래 queue suffix를 그대로 반환한다.

이전에 true로 확정한 칸이 나중의 false 대입 mask에 다시 포함되어도 안전하다.
그 변수는 이전의 **완전한** cofactor에서 residual 전체로부터 제거되었으므로 후속
제한은 아무 영향이 없다. 서로 겹치는 row가 같은 칸을 여러 번 true로 지정해도 같은
이유로 OR 점유 의미를 보존한다. 반대로 false로 고정한 칸은 당시 선택한 child의
실제 미래 지원에 없으므로 이후 row가 새로 그 칸을 제공하는 경우가 없다. 이 성질은
유효 geometry의 disjointness에 의존하지 않는다. 다만 겹친 row를 geometry 해로
허용한다는 뜻은 아니며, 기존 Down·rank 계약은 그대로 별도 적용한다.

ZDD의 생략된 row는 false다. child로 건너뛰면서 마지막 공급자가 사라진 cell은 위
지원 집합의 차집합에서 false로 처리된다. low branch에서 제외한 row의 칸이라도 다른
suffix row가 제공할 수 있으면 T(l)에 남으므로 성급하게 false로 지정하지 않는다.
BDD에서 생략된 cell 변수는 그 residual의 값에 영향을 주지 않으며, 실제 cofactor가
이미 이를 생략한 경우도 별도 mask에 기억할 필요가 없다.

따라서 서로 다른 앞 선택이 같은 `(z,p)`에 도달하면 모든 남은 suffix에 대한 답도
같다. 고정된 cell 순서의 canonical BDD는 동등한 residual을 같은 ID로 합칠 수 있어
이 재사용에 유리하다. memo에는 complemented edge의 극성을 포함한 전체 ID가 필요하다.
regular ID만 key로 사용해 함수와 부정을 합치면 안 된다. 제한과 부정은 교환하므로
기존 complement-aware cofactor 의미를 유지하면 음의 root도 같은 증명을 따른다.
source/Hold 상태, catalog·초기 보드·cell 순서와 arena owner/epoch는 여전히 고정된
호출 계약이며, 서로 다른 계약의 cache를 숫자 ID만으로 섞지 않는다.

기존 [`symbolic_family_parallel.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_parallel.rs)의
row 경로는 완성한 movement predicate를 `seal_for_reading()`으로 봉인한다. Eager
cofactor에 필요한 writable owner는 cell 경로의 consumer 전 봉인을 미루는 방법으로
해결했다. Cell worker는 private apply cache만 먼저 해제하고 unique table을 유지한다.
Consumer는 row별 checkpoint 이후 만든 residual을 성공·거절 모두에서 rewind하고,
사용을 마친 suffix는 drop한다. 기존 row 경로의 봉인은 유지된다.

Sealed base를 읽는 별도 residual owner 방식은 구현하지 않았다. 전체 base 복제도 하지
않는다. 지연 `(z,p,M)` 방식은 원래 predicate를 읽기만 할 수 있는 반면 eager 방식은
새 residual graph를 만들 수 있다는 실제 구현 차이는 그대로다.

이 방식은 명시적인 pending-mask 차원을 residual 함수 안으로 옮긴다. **상태 정보나
최악의 지수적 폭증을 없앴다는 뜻은 아니다.** 임의 위치의 cell 네 개를 즉시 대입하는
것만으로도 큰 BDD를 반복 방문하거나 새 cofactor node를 많이 만들 수 있다. 연속된
단일-variable 제한을 반복하지 않도록 `BooleanArena::cofactor_cells`에 여러 true/false
변수를 한 번에 제한하는 완전한 traversal을 구현했다. `force_cells_true`도 정확한
`(∃ cells . P) ∧ cube(cells=true)` 연산으로 구현되어 image 계산에서 사용한다.
이 구현 사실이 작업량 감소나 작은 residual을 보장하지는 않는다. `(z,p)` product의
p 범위는 원래 P의 node만이 아니라 새로 만들어지는 모든 residual이며, cofactor
cache와 임시 apply storage까지 포함해야 한다. GC를 적용한다면 traversal stack과
cache에 살아 있는 모든 residual root를 추적하고, remap 또는 epoch별 cache 무효화를
해야 한다.

## 5. 기존 계산 단위를 재사용할 위치

| 기존 단위 | 재사용할 부분 | 유지할 경계 |
| --- | --- | --- |
| `GeometryCatalog::skeleton`과 cell support | row→u64 cell mask, 가능한 suffix 지원, 초기·required mask | row identity와 piece kind를 제거하지 않음 |
| `SymbolicOccupancy::for_domain`의 줄 압축 DP | U→D/B, 최대 64개의 deleted-row frame | primitive cell 변수로 바꾸더라도 같은 좌표·초기 보드 계약 유지 |
| `placement_conditions::AtomicRowProjection` | 고정 frame에서 original/physical mask 투영 | concrete operation의 작은 u16 mask를 전역 catalog ID로 재사용하지 않음 |
| catalog realization cache | D별 pending row의 ILC pose와 lock mask | B만 같은 서로 다른 D의 row 답을 합치지 않음 |
| shared reachability template와 transfer descriptor | sky seeds, 하강·이동·ordered kick topology와 guard | SRS/SRS+/SRS-X/Jstris180/imported ordered authority를 그대로 바인딩 |
| `GeometryCompatibilityFilter` | 원래 row 집합의 Down(F), 필요한 cell-support traversal 패턴 | 현재 cell 경로도 원래 Down 판정을 유지함; §7의 더 강한 producer 계약 없이 그 판정을 U 또는 U·수량 image로 대체하지 않음 |
| `ZddArena::node/mk`, 기존 prefix semijoin | 원래 row/source relation을 공유하며 재구성 | joint suffix의 상관과 compaction epoch 유지 |
| `BooleanArena::node/cofactor_cells/force_cells_true` | 구현된 complement-aware 완전 제한과 OR 점유 image 연산 | eager 제한에는 writable residual owner가 필요하며 cell 경로의 apply-cache 해제·checkpoint/rewind 경계를 유지 |

최종 cell legality는 physical lock·ILC realization과 `C_r ∩ U = ∅`를 함께 포함한다.
catalog row는 비어 있지 않으며 선택한 row의 칸은 줄 삭제 후에도 원래 좌표의 U에
남는다. 따라서 r가 이미 선택되었으면 이 조건은 반드시 거짓이다. **추가적인
row-variable `!selected(r)` predicate는 필요하지 않다.** 이 cell predicate를 원래
joint ZDD에 정확히 역필터한 뒤 row를 추가하고 원래 geometry membership을 검사한다.
서로 다른 분해가 같은 U를 만들더라도 원래 row 집합은 별개의 해법 identity로 남는다.
source/Hold/EOF 상태 key도 기존처럼 유지한다.

## 6. 비용과 우선순위

cell 수가 작아져도 지연 `(z,p,M)` 상태 수는 커질 수 있다. 거친 상한에는 `2^area`
종류의 pending mask가 들어가며, 특히 앞 cell의 참/거짓 결정을 뒤까지 미뤄야 하는
row 순서에서는 memo 공유가 약해질 수 있다. eager `(z,p)`도 원래 predicate보다 많은
residual 함수와 cofactor 작업을 만들 수 있다. 실제 physical image 또는 상위 domain의
BDD 크기, suffix-support 비용, 역필터 memo, 최종 joint ZDD를 모두 계측해야 한다.
명시적 완성 후보 iterator가 없다는 것과 상태 폭증이 없다는 것은 다른 주장이다.

현재 구현과 유한 검증을 완료한 범위는 다음과 같다. 구체적인 test 로그와 남는 한계는
별도 구현 보고서에 연결했다.

| 항목 | 확인한 상태 |
| --- | --- |
| Exact cell image와 eager 역필터 | 구현 및 작은 전체 truth oracle·source pair fixture 통과 |
| Initial/overlap/zero suppression/complement | Bridge·Boolean fixture 통과 |
| Owner/epoch support 재사용과 residual 수명 | 다른 arena 동일 ID·compact epoch·parallel owner·거절 fixture 통과 |
| 같은 B에 다른 D가 필요한 4×3 ILC 예 | Executable fixture 통과 |
| 기존 kick·Hold/EOF·source 경계 | 선택한 family suite 회귀 통과; Full4 전체 결과 검증은 별개 |
| Safe-superset domain과 lazy 역필터 | 미구현 대안 |

### P0 — 실제 실행과 결과 검증

현재 두 cell 후보의 source/binary, 실행 경계와 미완료 결과는
[구현 보고서 §8·§9](symbolic-cell-occupancy-2026-11-23.md)에 기록했다. Row별 Down은
444.039초, piece별 Down은 285.243초에 signal 9로 끝났으며 둘 다 rank 3 미완료다.
완료 시간의 비교나 전체 해법 일치 검산으로 취급하지 않는다.

- [ ] 수정 파일: `crates/clearra-core-executor/examples/symbolic_family_compare.rs`, `docs/research/symbolic-cell-occupancy-2026-11-23.md`, 구현 내용: 구조적으로 개선한 후보가 Full4를 완료하면 원래 identity·source·joint 검증 범위를 나눠 대조하고 실제 완료 결과를 기록한다.

### P1 — 측정에 따른 대안 검토

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_cell_bridge.rs`, 구현 내용: exact image·eager cofactor·support cache의 실제 비용을 분리해 확인하고, 필요하면 safe-superset 또는 lazy 역필터를 독립 정확성·수명 fixture와 함께 구현한다.
- [ ] 수정 파일: `crates/clearra-core-executor/examples/symbolic_family_compare.rs`, 구현 내용: 필요한 추가 계측이 확인되면 residual graph·product memo·임시 allocation을 실제 RSS와 구분해 기록하며 기존 work를 중복 합산하지 않는다.

### P2 — 큰 입력 판단

- [ ] 수정 파일: `docs/research/symbolic-cell-occupancy-2026-11-23.md`, 구현 내용: Full4가 60초 미만에 전체 탐색을 성공한 뒤 Full6로 확장하고, 원래 joint 결과의 검증 범위와 source/binary·환경 근거를 함께 기록한다.

## 7. 후속 설계: 점유와 미노 수량으로 geometry 연장 가능성 판정

**이 절은 아직 구현하지 않은 설계다.** 현재 cell movement와 exact 역필터, 그리고
piece 단위로 기존 Down 검사를 모으는 변경에는 아래의 강화된 producer 계약이
필요하지 않다. 여기서는 원래 geometry Down 검사 자체를 대체할 수 있는 조건을
따로 다룬다. 수량 벡터 \(\boldsymbol M\)은 §4의 lazy pending-cell mask M과 다르다.

### 7.1 점유만으로 부족한 정보를 미노별 수량으로 보완하는 조건

초기 점유 I와 required region R은 서로 겹치지 않는다. 고정된 catalog의 모든 row는
비어 있지 않은 \(C_r\subseteq R\)과 고정 piece kind를 갖는다. S는 그 catalog에서
고른 서로 겹치지 않는 row 집합이고, 다음과 같이 정의한다.

\[
\Phi(S)=\left(U(S),\boldsymbol M(S)\right),\qquad
\boldsymbol M(S)=\sum_{r\in S} e_{\operatorname{kind}(r)}\in\mathbb N^7.
\]

이 수량은 **배치한 미노**를 센다. Hold에 남은 미노나 읽었지만 배치하지 않은 draw를
포함하지 않는다. U는 삭제·압축 전 원래 좌표의 점유다. 물리 B만 남기면 §2의 frame
반례가 다시 발생한다.

허용하는 최종 수량 집합을 T라 하자. 필요한 충분조건은 F가 다음 집합 **전체**라는
것이다.

\[
F_T(R)=\left\{C\subseteq\mathrm{Catalog}:
 C\text{의 footprint들이 서로 겹치지 않고},
 \bigcup_{r\in C}C_r=R,\quad \boldsymbol M(C)\in T\right\}.
\]

이 조건 아래에서는 \(\Phi(S)=\Phi(S')\)이면
\(S\in\operatorname{Down}(F_T(R))\iff S'\in\operatorname{Down}(F_T(R))\)이다.
이를 확인하려면 \(S'\subseteq C\in F_T(R)\)인 완성을 하나 잡으면 된다.
\(C\setminus S'\)는 정확히 \(R\setminus U(S')\)를 채운다. S와 S'의 점유가 같으므로
이 나머지는 S와도 겹치지 않으며, 수량도 같으므로
\((C\setminus S')\cup S\)는 동일한 최종 수량의 exact cover다. F가 이 조건을 만족하는
모든 cover를 포함하므로 바꿔 넣은 cover도 F에 있다. 역방향도 대칭이다.

이는 같은 점유·수량의 부분 배치를 교환해도 남아 있는 **swap closure**다. row의
순서를 바꿀 수 있다는 permutation closure나 각 원소가 valid cover라는 성질보다
강하다. 같은 \(\Phi\)를 가진 해법의 row identity가 같다는 주장은 아니다.

### 7.2 임의의 family에 적용할 수 없는 반례와 producer 권한

빈 4×4 보드에서 catalog에는 수평·수직 I row가 모두 있다고 하자. F를 네 개의
수평 I로 채운 cover 하나만 포함하도록 제한한다. S는 그 수평 네 row, S'는 수직
네 row를 고른 집합으로 둔다. 두 집합 모두 U=보드 전체이고 수량은 I 네 개지만,
S는 Down(F)에 있고 S'는 없다. 그런데 두 집합의 \(\Phi\)는 같다. 따라서
Down(F)의 \(\Phi\)-image를 정확히 계산해도, 임의 family에 대한 역방향 membership
oracle로 쓰면 거짓 양성이 생긴다.

현재 [`certify_exact_cover`](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_compatibility_filter.rs)는
Append의 cell 비겹침, Union의 동일 region, Product의 서로 겹치지 않는 region과
최종 required region을 검사한다. 위의 수평 cover 하나도 이 검사를 통과한다.
**이 인증은 F가 모든 cover를 포함한다는 인증이 아니다.** 외부에서 받은 후보 일부,
최종 수량 외에 source의 순서·위치 또는 Build 순서로 개별 cover를 제한한 family나
arbitrary Union에는 그 권한을 자동으로 부여하면 안 된다.

현 [`compile_symbolic_geometry`](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs)는
이 더 강한 권한을 검토할 producer 경계다. 이 함수는 target multiset key별로
`PatternBitSet::all(1)`과 `pattern_index: None`을 만들고, `FamilyCompiler::try_new`는
그 target들의 모든 submultiset prefix를 허용한다. `completed_target`은 최종 수량
key를 검사한다. 빈 P7P4에서는 `advanced_domain`이 꺼져 있으며, 공통
[`cheap_residual_impossible`](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_projection.rs)는
catalog row의 가산 불변량에 근거한 필요조건이다. 영역 분해의
[`compile_component_plan`](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_component.rs)은
local 한도를 넘으면 일반 compiler로 돌아간다. 이 경로들은 source 위치나 실제
배치 순서를 geometry family의 조건으로 넣지 않는다.

미래 구현에서는 이 코드 검토를 단순 enum 이름 확인으로 대체하면 안 된다.
완료된 full producer만 만들 수 있는 별도 권한을 catalog의 cell·kind mapping,
required/initial mask, 전체 T, family root 및 producer 계약에 묶어야 한다. 활성화한
모든 pruning·component·memo 경로가 이 전체 집합을 보존한다는 근거도 포함해야 한다.
취소·자원 거절·부분 생성은 권한을 반환하지 않는다. 권한이 없는 family는 기존
row-set Down 검사를 유지한다. 그 경우 \(\Phi\)-image를 쓰더라도 음성 가지치기에만
쓰고, 양성 결과에는 원래 검사를 남겨야 한다.

### 7.3 원본 DAG의 region-local image와 수량 convolution

node g의 모든 cover가 채우는 고정 region을 \(W_g\)라고 하자. 위 구조 인증이
Append·Union·Product의 region 관계를 제공한다. 다음 함수는 node 아래 cover의
임의 부분집합을 선택했을 때 가능한 **점유와 선택 수량의 image**다.

\[
G_{g,\boldsymbol m}(X)
=\exists C\in F_g,\ S\subseteq C:
  \bigcup_{r\in S} C_r=X\subseteq W_g,\quad
  \boldsymbol M(S)=\boldsymbol m.
\]

각 함수의 cell domain은 \(W_g\) 내부다. 자식 함수를 부모 scope에 올릴 때 자식
region 밖의 cell을 FALSE로 고정하지 않고 **미지정 변수**로 둔다. \(Z_A\)와 \(O_A\)는
각각 A의 모든 칸이 0 또는 1이라는 cube이고, 음수가 된 수량 slice는 FALSE다.

| DAG node | Region-local predicate |
| --- | --- |
| INVALID | 항상 FALSE |
| UNIT | 빈 region에서 \(\boldsymbol m=0\)일 때만 TRUE |
| Append(r,c) | \((Z_{C_r}\land G_{c,\boldsymbol m})\lor(O_{C_r}\land G_{c,\boldsymbol m-e_{kind(r)}})\) |
| Union(a,b) | \(G_{a,\boldsymbol m}\lor G_{b,\boldsymbol m}\); 두 자식 region이 같음 |
| Product(a,b) | \(\displaystyle\bigvee_{\boldsymbol u+\boldsymbol v=\boldsymbol m}(G_{a,\boldsymbol u}\land G_{b,\boldsymbol v})\); 두 자식 region이 서로 겹치지 않음 |

Append는 해당 row를 고르지 않거나 통째로 고르는 경우다. region 비겹침 때문에
다른 자식 row가 그 칸을 대신 채우지는 않는다. Product에서는 각 자식의 수량을
**합성곱으로 더해야** 한다. 두 자식에 같은 수량 값을 요구하는 AND는 잘못된
연산이다. Union은 Boolean OR이므로 중복 derivation을 한 번으로 합친다. 원본
`path_count`의 덧셈·곱셈은 이 image의 identity 개수나 source 개수로 사용하지 않는다.

Scope 처리를 빼면 Product도 틀린다. 예를 들어 왼쪽은 cell a만, 오른쪽은 cell b만
선택할 수 있을 때, 각각을 whole-board 함수 `a ∧ !b`, `!a ∧ b`로 만들면 AND가
FALSE다. region-local 함수 a와 b를 부모 scope로 올리면 AND는 올바른 `a ∧ b`다.
Whole-board characteristic image를 유지하려면 AND 대신 별도의 disjoint-union
image 연산이 필요하다. UNIT의 TRUE도 전역 보드가 비었다는 뜻이 아니다.
최상위에서만 required region 밖이 고정 I와 일치한다는 원래 catalog 경계를 적용한다.

이 recurrence는 구조 인증된 임의 F의 image에는 정확하다. 그 image를 원래 row-set
Down의 정확한 membership으로 되돌릴 수 있다는 결론에는 §7.1의 full producer
조건이 추가로 필요하다. 두 인증의 역할을 합치지 않는다.

### 7.4 원래 joint relation으로 돌아오는 경계와 비용

원래 상태의 수량이 \(\boldsymbol M\)일 때, row r의 전이를 다음 조건으로 묶는
형태를 검토할 수 있다.

\[
\operatorname{Legal}_r(U)\ \land
G_{root,\boldsymbol M+e_{kind(r)}}\bigl((U\setminus I)\cup C_r\bigr).
\]

Legal에는 이미 \(C_r\cap U=\varnothing\)가 들어 있다. 두 번째 항은 r 추가 이후에도
full geometry family로 연장 가능한지 판정한다. source/Hold 상태별 원래 joint ZDD를
이 조건으로 **정확히** 필터한 다음 row를 추가한다. 서로 다른 row 분해나 원래 q를
합치거나, 같은 U·수량의 하나를 대표 해법으로 고르지 않는다. CountUnique·CTK4·coverage
계산도 원래 joint relation에 남긴다. 이 image는 predicate oracle이다.

현재 `CellFamilyBridge`는 cell predicate만 받는다. 위 식의 수량 의존성을 지원하려면
필요한 수량 slice별로 원래 row relation을 정확히 분리하거나, fused traversal에 남은
수량 문맥을 포함하는 별도 계약이 필요하다. source/Hold 상태 하나의 `consumed` 값이나
rank만으로 미노별 수량을 대체하면 안 된다. ZDD의 생략 row는 미선택이며, source
suffix는 원래 상관관계를 유지한다. 이 추가 연산은 현재 bridge에 구현되어 있지 않다.

필요한 rank·수량 slice만 demand-driven으로 계산하면 모든 rank의 Down image를
먼저 만들지 않을 수 있다. 그래도 `(family node, 수량 slice)`, cell-BDD, 수량
convolution, 역필터의 residual과 원래 joint ZDD가 각각 커질 수 있다. 열 단위
frontier 설계도 폭 4칸의 미노에는 최대 세 개의 다음 열과 수량·걸친 배치 의무를
보존해야 하므로 상태가 단순히 H개 bit라고 할 수 없다. 완성 후보 열거를 없앤다는
것은 다항 시간이나 제한된 메모리를 보장한다는 뜻이 아니다. 취소·work/node 제한·할당
거절은 기존과 같이 빈 결과와 구분하고, owner/epoch 및 임시 peak를 계측해야 한다.

### P0 — geometry quotient 도입 전 필요한 구현·검증

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs`, 구현 내용: 전체 catalog·target multiset에 대한 완료 producer만 발급하는 swap-closure 권한을 정의하고, target 수량 외 source·순서 조건으로 제한한 family 및 부분 생성에서 발급되지 않도록 검증한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_compatibility_filter.rs`, 구현 내용: 기존 region 인증과 full producer 권한을 구분하고, 4×4 수평/수직 I 반례에서 구조 인증만으로 양성 판정을 대체하지 못하도록 회귀를 추가한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_cell_bridge.rs`, 구현 내용: 필요성이 계측으로 확인되면 region-local 수량 image와 정확한 수량 문맥 역필터를 구현하고, Product scope·convolution·zero suppression·original identity×source 관계를 유한 oracle과 비교한다.
- [ ] 수정 파일: `docs/research/symbolic-cell-occupancy-2026-11-23.md`, 구현 내용: 실제 도입 후보에서 geometry·image·역필터 비용과 전체 joint 결과를 각각 검증한 뒤 채택 여부를 기록하며, 이 설계를 현재 구현의 성능 근거로 사용하지 않는다.

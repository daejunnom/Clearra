# Full 6L: 완성 후보 전개를 제거하는 family 집합 전이

작성일: 2026-11-23 KST. 검토한 제품 소스: `cf8af10152bb5c802400f87a9b8a7b9531e89c7d`.

상태: 코드 읽기, 원본 연구 자료 확인, 수학적 설계 및 독립 반례 검토를 마쳤다.
아래 새 탐색기는 아직 구현하지 않았고 성능·메모리 수치를 측정하지 않았다.
테스트, Full 6L 탐색, CI, 배포를 실행하지 않았다. 사용자가 제공한 100% 확률은
확정 전제로 두었고, 이를 재검색·재계산·재증명하지 않았다.

## 1. 설계 결정

다음 단계의 주된 목표를 **후보마다 Build를 더 싸게 수행하기**에서
**완성 후보를 생성하지 않고 실행 가능한 타일링 family를 직접 계산하기**로 바꾼다.

추천하는 방법은 원본 geometry family F의 부분집합 폐포 Down(F)를 기하적
완성 가능성 제약으로 사용하고, 실제로 도달 가능한 부분 배치 family L_k를
물리·공급 전이로 갱신하는 것이다. 완성 후보 C를 각 상태에 함께 보관하지 않는다.
같은 부분 배치가 수많은 C에 포함되더라도 그 배치에 대한 물리 계산을 C마다
반복할 필요가 없다.

이 제안은 기존 DAG의 마지막 leaf에서 verifier를 호출하는 필터가 아니다.
결과도 accepted family로 보존하고, 정확한 Unique count와 요청한 일부 witness만
꺼내야 한다. 최종 identity 전체를 다시 Vec/HashSet으로 펼치는 경로까지 바꾸는
것이 설계 범위다.

다만 다음 두 결론은 구별한다.

- 후보별 전개와 후보별 Build를 알고리즘의 필수 단계에서 제거할 수 있다.
- Full 6L의 후속 decision diagram이 충분히 작다는 보장은 아직 없다.

두 번째를 확인하지 않고 Full 6L을 실용 시간 안에 해결했다고 주장하지 않는다.

## 2. 현재 DAG가 실제로 압축하는 것

[geometry_family.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_family.rs)는
다음 집합 수식을 구조적으로 공유한다.

| 표현 | 집합 의미 |
| --- | --- |
| `FAMILY_INVALID` | 공집합 ∅ |
| `FAMILY_EMPTY` | 빈 선택 하나로 된 집합 {∅} |
| `Append(r,A)` | {a∪{r}: a∈A} |
| `Union(A,B)` | A∪B |
| `Product(A,B)` | {a∪b: a∈A, b∈B} |

여기서 r은 보드 행 번호가 아니라 catalog의 **미노 종류와 원본 footprint를
식별하는 row-ID**다. Full 6L의 완성 타일링들은 같은 10×6 영역을 채우지만,
그 영역을 미노로 분할하는 방법이 다르다. 따라서 과거의
247,794,216,021,372 paths를 서로 다른 최종 점유 비트보드 수나 물리 해법 수로
해석하면 안 된다. 이 수와 52,325,237 family nodes는
[기존 실행 기록](full6l-p1p7p7-2026-11-22.md)의 수치이며 이번에 재실행하지 않았다.

일반 Family API는 canonical ZDD가 아니다. `(kind,left,right,row_id)`가 같은
구조를 공유하며, `path_count`는 Union에서 더하고 Product에서 곱한다.
일반 API에서 이 값은 derivation multiplicity다.

반면 현재 exact-cover compiler에는 더 강한 생성 불변식이 있다.

- 한 잔여 영역에서 pivot 셀을 덮는 row 하나를 선택하므로 Union branch가 서로
  다른 exact cover를 만든다.
- 선택한 4셀을 제거하므로 row 중복과 셀 겹침이 없다.
- component는 feasible row가 가로지르는 셀을 같은 영역으로 묶는다. Product의
  owner/remainder가 분리되어 각 완성 cover의 분해가 유일하다.
- 요구 셀 수가 4N이면 완성 cover는 모두 N개의 row-ID를 가진다.

이 compiler 경로의 원본 paths를 고유한 **기하 cover** 수로 읽을 근거는 있다.
그러나 임의의 필터·투영·Down 변환 후에도 이 count 권한이 자동으로 유지되지는 않는다.
생성 근거는 [geometry.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs),
[geometry_component.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_component.rs),
[기존 uniqueness 감사](tsar-half-geometry-bottleneck.md)에 있다.

### 2.1 node당 참·거짓 하나로 해결할 수 없는 이유

동일한 suffix node도 앞서 선택하거나 배치한 미노, 줄 삭제 frame, source/Hold에
따라 물리 결과가 달라진다. node만 보고 Boolean을 memo하면 이 문맥을 잃는다.
반대로 `(node, 완전한 concrete prefix)`를 모두 memo하면 큰 전개를 캐시 구성으로
옮길 수 있다.

Append의 기하 생성 순서는 물리 lock 순서가 아니다. Product도 두 영역을 순서대로
실행한다는 뜻이 아니다. 이동·지지·전역 행 삭제·같은 큐와 Hold가 영역들을 결합한다.
두 자식의 Boolean 존재성만 곱하면, 서로 다른 문맥에서만 가능한 두 선택을 잘못
연결할 수 있다. 필요한 것은 같은 문맥 z에 대한 `∃z: A(z)∧B(z)`이지
`(∃z:A(z))∧(∃z:B(z))`가 아니다.

## 3. 완성 cover를 상태에서 제거하는 방법

catalog row-ID 우주를 R, 완성 family를 F⊆{C⊆R:|C|=N}이라 하자.

\[
\operatorname{Down}(F)=\{S\subseteq R:\exists C\in F,\ S\subseteq C\}.
\]

이는 **어떤 완성 cover로 확장할 수 있는 부분 선택들**의 집합이다. 아직 물리적으로
배치 가능하다는 뜻은 아니다. 물리 도달성은 다음 절의 L_k가 따로 보존한다.

### 3.1 DAG에서 만드는 부분집합 폐포

\[
\begin{aligned}
\operatorname{Down}(\varnothing)&=\varnothing,\\
\operatorname{Down}(\{\varnothing\})&=\{\varnothing\},\\
\operatorname{Down}(\operatorname{Append}(r,A))
&=\operatorname{Down}(A)\cup\operatorname{Append}(r,\operatorname{Down}(A)),\\
\operatorname{Down}(A\cup B)&=\operatorname{Down}(A)\cup\operatorname{Down}(B),\\
\operatorname{Down}(A\otimes B)&=\operatorname{Down}(A)\otimes\operatorname{Down}(B).
\end{aligned}
\]

마지막 곱은 `{a∪b}`의 조건 없는 집합 합성을 뜻한다. Clearra compiler가 인증한
비중첩 component Product에도 적용할 수 있다. 호환되는 완성 원소 쌍만 선택하는
일반적인 filtered product에는 그대로 적용하지 않는다. 완성 원소끼리는
비호환이어도 Down 이후 빈 집합끼리는 호환될 수 있기 때문이다. 원본의 호환
piece-signature 쌍별 `Union(Product(...))` 구조를 보존하고, 서로 다른 branch의
Down operand를 새로 교차 곱하지 않는다.

같은 자식을 공유하면 이 **집합 수식의 회로 표현**은 원본 node마다 O(1)개의
추가 node로 표현할 수 있다. canonical decision diagram으로 바꾸는 비용까지
선형이라는 뜻은 아니다. 구현에서는 전체 Down DAG를 복사하지 않고, 원본의
immutable family에 대한 가상 view와 필요한 부분의 fused apply를 우선 검토한다.

### 3.2 정확한 층별 전이

q를 원본 source의 한 큐, a를 현재 draw 위치·Hold·EOF 등 공급의 정확한 잔여 상태로
둔다. 원본별 coverage가 필요하면 q와의 관계를 유지한다.

\[
L_0=\{(\varnothing,q,a_0(q)):q\in Q,\ \varnothing\in\operatorname{Down}(F)\}.
\]

\[
L_{k+1}=\left\{(S\cup\{r\},q,a'):
\begin{array}{l}
(S,q,a)\in L_k,\quad r\notin S,\\
\operatorname{Legal}_r(S),\\
\operatorname{SupplyStep}(q,a,\operatorname{kind}(r),a'),\\
S\cup\{r\}\in\operatorname{Down}(F)
\end{array}\right\}.
\]

`SupplyStep`은 기존 source/Hold/EOF의 전이다. Hold가 빈 경우와 lookahead 때문에
lock 개수 k와 소비한 draw 개수는 같다고 가정하지 않는다.

F가 원본 q에 따라 별도의 허용 조건을 갖는 일반화에서는
`Γ(S,q)=∃C: F(C,q)∧S⊆C`를 사용한다. 순수 기하 제약과 정확한 실제 공급 전이로
충분한 현재 대상에서만 Γ를 Down(F)로 줄일 수 있다. L_k를 S 집합과 q 집합으로
각각 투영한 뒤 다시 곱하면 서로 다른 원본의 witness를 섞으므로 허용하지 않는다.
종료 판정에는 [terminal_hold_projection.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/terminal_hold_projection.rs)의
실제 finite EOF·current 존재·추론 lookahead 조건도 포함한다.

L_k 자체를 symbolic relation으로 유지한다. 위 식을 concrete S의 HashMap 순회로
구현하면 의도한 구조 변경을 완수하지 못한 것이다. Full 6L에서는 N=15개의 집합
전이 층이 있지만, 이것은 CPU 연산 15번이라는 뜻이 아니다. 각 층의 symbolic image
연산량은 생성되는 관계와 중간 표현의 크기에 좌우된다.

### 3.3 건전성과 완전성

**건전성.** L_k의 원소는 귀납적으로 실제 source/Hold 전이와 물리 lock k회를 거쳐
도달한다. terminal 공급 조건을 만족하는 L_N의 S는 Down(F)에 속하고 |S|=N이다.
따라서 S⊆C인 어떤 C∈F가 있으며, |C|=N이므로 S=C다. 실제로 실행 가능한 원본
완성 cover만 남는다.

**완전성.** 실제로 가능한 C∈F의 실행 순서를 r_1,…,r_N이라 하자. 모든 prefix S_k는
C의 부분집합이므로 Down(F)에 속한다. 각 실제 물리·공급 전이가 recurrence에
포함되므로 그 실행의 모든 prefix와 terminal C가 L_k에 포함된다.

각 단계의 completion witness C가 달라도 문제가 없다. S가 단조 증가하므로
최종 C는 앞선 모든 S를 포함한다. 따라서 완성 후보에 대한 commitment를 계속
상태에 보관할 필요가 없다.

이 증명은 해당 ILC 모델과 출력 계약의 알고리즘 동치에 관한 것이며, 사용자 제공
확률값을 재증명한 것이 아니다. 가변 크기 family라면 |S|=N 규칙 대신 정확한
terminal F membership을 사용해야 한다.

## 4. 부분 선택만으로 ILC 물리 상태를 결정할 수 있는가

고정 initial board O와 original footprint M_r에 대해

\[
U(S)=O\cup\bigcup_{r\in S}M_r,\qquad
D(S)=\operatorname{FullOriginalRows}(U(S)),\qquad
B(S)=\operatorname{Compact}(U(S),D(S)).
\]

현재 [CandidateProjection](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs)의
`state`와 `expected_deleted_rows`가 사용하는 의미가 바로 이것이다. 유효한 original
cover와 초기 full-row 정규화를 전제로, B와 D는 완성 C나 배치 순서가 아니라 S로
결정된다. 기하적으로 가능한 임의 S가 실제 도달 가능하다는 주장은 아니며,
도달성은 L_k가 유지한다.

미배치 r이 차지할 원본 셀은 아직 채워지지 않았다. 비중첩 exact cover에서는
그 셀을 포함한 원본 행을 r 없이 먼저 완성할 수 없다. 따라서 삭제 frame의
일관성을 유지한 채, gap이 있는 ILC skeleton도 현재 D(S)에서 realization을
구해 검증할 수 있다. 이전 rigid-column 인증처럼 frame-zero 형상만으로 범위를
제한할 필요가 없다.

Symbolic한 셀 점유와 행 삭제는 다음 식으로 표현할 수 있다.

\[
u_{x,y}(S)=o_{x,y}\lor\bigvee_{r:(x,y)\in M_r}s_r,
\qquad d_y(S)=\bigwedge_{x=0}^{W-1}u_{x,y}(S).
\]

남는 원본 행 y의 현재 높이는 `y−popcount(D below y)`다. 이 rank/select 관계와
카탈로그의 realization 조건을 사용한다. 모든 삭제 frame을 먼저 열거한 뒤 표로
만드는 방식은 필수 단계가 아니다.

### 4.1 `Legal_r`에 반드시 남겨야 할 것

- 선택한 r의 종류와 **전체 원본 footprint**.
- D(S)에 맞는 실제 tetromino realization.
- B(S)에서 충돌 없음과 정확한 접지.
- 허용 entry에서 그 pose까지의 정확한 이동 도달성.
- 회전에서는 앞 kick 실패와 선택 kick 성공의 순서 있는 조건.

유효한 source pose p와 회전 target p_j에 대한 전이 family는

\[
E_{p,j}=Free_p\cap Free_{p_j}\cap\bigcap_{i<j}\neg Free_{p_i}
\]

다. 기존 normalized kick owner와 이 집합 의미를 재사용하면 SRS, SRS+, SRS-X,
Jstris 180 및 지원되는 imported table을 profile 이름별 특례 없이 다룰 수 있다.
finesse·spin·입력 이력까지 요구하는 요청은 해당 상태를 추가해야 하며 Boolean PC
존재성 모델로 몰래 대체하지 않는다. see-7처럼 관측에 따른 전략의 양화 순서가
다른 문제도 이번 full-queue existential 설계에 자동 포함하지 않는다.

[reachability_board_family.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_board_family.rs)와
[기존 보드 집합 연구](combinatorial-board-family-proof-2026-11-22.md)는 pose마다
도달 가능한 보드 집합을 유지하는 기반을 제공한다. 현재는 `cfg(test)`이고,
[BooleanFamily](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_boolean_family.rs)는
최대 64변수 검산기다. 새 S의 변수 우주는 candidate-local 15개가 아니라 전체
catalog row-ID이므로 이 구현의 cfg만 바꾸어 제품에 넣는 것으로는 충분하지 않다.

## 5. 병목을 다른 곳에 만들지 않는 실행 구조

### 5.1 큰 전역 legality diagram을 먼저 만들지 않는다

`F ∩ Legal`이라는 식만 쓰고 모든 보드·모든 subset·모든 source에 대한 Legal을
선계산하면 같은 어려움을 전처리로 옮길 수 있다. 현재 L_k와 살아 있는 기하 제약에서
실제로 필요한 관계만 결합한다.

각 미노/행/이동 guard의 전이를 분할하고, 결합과 존재 양화를 한 연산에서 수행한다.
어떤 임시 변수가 남은 조건에서 더 읽히지 않을 때만 제거한다. 공유 변수의 모든
조건을 적용하기 전에 각각 양화하면 서로 다른 보드나 큐의 witness를 섞을 수 있다.
보드 도달성의 양성 집합을 재사용할 때도 인증한 domain과 rule/table 소유권을 보존한다.

이 방향은 Burch·Clarke·Long의 partitioned transition relation과 early quantification
원리를 적용한 설계다. 그 연구는 큰 단일 전이 BDD와 전체 중간 conjunction을 먼저
만드는 비용을 줄인다. Clearra에서 같은 효과가 난다는 실측 근거로 사용하지 않는다.
[원문](https://www.cs.cmu.edu/~emc/papers/Conference%20Papers/Representing%20Circuits%20More%20Efficiently%20in%20Symbolic%20Model%20Checking.pdf)

### 5.2 무엇을 소거하고 무엇을 보존하는가

| 정보 | 처리 |
| --- | --- |
| 완성 후보 C/ordinal | Down 제약 안에서 조기 소거; 각 상태에 carry하지 않음 |
| 중간 pose·보조 회전 변수 | 해당 전이의 마지막 사용 후 소거 |
| 이전 상태의 임시 복사 | 새 상태와 관계를 결합한 후 소거 |
| 최종 타일링을 구별하는 row 선택 | CountUnique를 위해 압축 provenance로 보존 |
| 원본 source q | pointwise coverage·원본 witness가 필요하면 관계를 보존 |
| 물리 board와 삭제 행 | S에서 유도하되 읽기와 재사용 비용에 맞춰 공유 |

이미 삭제된 행의 미노 선택을 전부 잊어도 된다는 뜻이 아니다. 물리적으로 같은
현재 보드에 도달한 다른 타일링은 현재 CountUnique에서 다른 해법일 수 있다.
미래 실행은 공유하되 과거 선택의 family는 보존해야 한다.

### 5.3 representation을 고르는 기준

희소한 row-ID 선택 family에는 ZDD 계열, 점유·회전 guard에는 Boolean 조건,
source에는 기존 잔여 언어가 자연스럽다. 그러나 representation을 늘리는 것 자체는
개선이 아니다. 변환을 매 전이에서 반복하거나 각 표현에 전체 집합을 복제하면 안 된다.
처음에는 정확한 set 의미를 갖는 한 공통 relation 연산 경계와 원본 DAG view를
정의하고, 다른 표현은 측정 가능한 이점이 있을 때만 채택한다.

Darwiche·Marquis의 compilation map은 작은 표현, 저렴한 count, 저렴한 conjunction,
저렴한 변수 소거가 서로 다른 성질임을 보여 준다. 특히 다수 변수 소거까지 항상
다항 크기로 끝난다고 가정할 수 없다.
[원문](https://www.cril.univ-artois.fr/~marquis/darwiche-marquis-jair02.pdf)

기하적으로 나뉜 Product도 물리적으로 독립이라는 인증이 없으면 별도로 풀어
곱하지 않는다. 작은 경계 상태로 정확히 합성 가능한 경우에만 factorization한다.
경계에는 공유 행 삭제, 이동 guard 읽기, 공급 상태가 포함될 수 있다. 보드 폭이
10이라는 사실만으로 이 전체 경계가 10비트라고 결론 내릴 수 없다.

### 5.4 구현에 금지할 우회

| 구현 방식 | 판정 |
| --- | --- |
| leaf에서 기존 `verify_candidate` 호출 | 후보별 병목이 유지됨 |
| 완성 C를 포함한 모든 `(C,S)`를 생성 | 기존 Build 상태공간을 다른 표에 옮김 |
| concrete S 전체를 HashMap에 넣고 DP | 후보 공유는 개선될 수 있으나 집합 압축 목표는 미완료 |
| 거대한 subset·보드 assignment를 legality 전처리로 생성하거나 후보마다 같은 표를 재구축 | 전개 비용의 이전 |
| 원본 5천만 node를 모두 canonical ZDD로 변환한 뒤 시작 | 변환 중간 크기를 확인하기 전 기본 전략으로 채택하지 않음 |
| accepted cover를 전부 펼쳐 dedup/count | 출력 단계로 병목 이전 |
| node/work/memory 거절을 empty set으로 반환 | 정확성 위반 |

작게 인증된 공통 표까지 금지하는 것은 아니다. 예를 들어 H=6에서 요청 전체가
공유하는 최대 64개 row-frame 표는 합당할 수 있다. 매 후보마다 반복하는 전개와
구분하고, 실제 생성 상태 수·재사용·전체 peak 비용을 기준으로 선택한다.

## 6. 중복 제거와 결과 API도 함께 바꾼다

원래의 deterministic Union도 Down 뒤에는 겹친다. 예를 들어

\[
F=\{\{a,b\},\{a,c\}\}
\]

이면 두 branch의 Down 모두 ∅와 {a}를 가진다. 여러 물리 순서와 Hold 선택도 같은
S에 도달할 수 있다. 따라서 L_k는 derivation 수가 아니라 **집합의 합집합**이어야
하며, 기존 `path_count`의 +/×를 Unique count에 재사용하면 안 된다.

원본 q와의 관계를 보존하고 terminal 상태 a의 중복 witness를 합친 accepted
relation을 다음처럼 정의한다.

\[
E(S,q)=\exists a:\ L_N(S,q,a)\land\operatorname{Terminal}(q,a).
\]

그 뒤 두 출력은 다음 투영으로 구한다.

\[
H(S)=\exists q\ E(S,q),\qquad G(q)=\exists S\ E(S,q)
\]

다. H는 실행 가능한 고유 타일링 family이고 G는 요청이 요구하는 원본 큐 coverage다.
둘은 다른 투영이며 `|E|`, 실행 순서 수, `|H|`도 구별한다. 이미 알려진 확률을
다시 계산하자는 제안이 아니다. 기존 API가 요청하는 출력 의미를 조용히 생략하지
않기 위한 구분이다.

같은 catalog에서는 `(piece,cells)`를 하나의 row-ID로 intern하고,
[StandardBoard64TilingIdentity](../../crates/clearra-core-domain/src/solution/normalized_tiling_solution.rs)는
그 pair들을 정렬 보존한다. 따라서 완성 row-set과 현재 Unique identity 사이에
일대일 대응이 있다. 같은 색의 셀을 합쳐 그리는 Fumen은 더 거친 표현이라 이를
대신 identity로 사용하지 않는다.

현재 [result.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/result.rs)와
[parallel_worker.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/parallel_worker.rs)는
요청에 따라 전체 identity를 Vec/HashSet으로 모으고 정렬·중복 제거한다. 새 경로는
압축 해법 family의 owner/root, 정확한 count, completeness, 제한된 page/witness를
결과로 다루는 계약이 필요하다. 기존 API의 의미는 명시적인 새 결과 형태로 연결한다.

기존 정렬 identity의 `cts1`/FNV digest도 자동으로 symbolic해지는 것은 아니다.
새 DAG의 root hash를 기존 digest와 같다고 보고하지 않는다. 해당 digest를 정확하게
합성하는 별도 연산을 제공하거나, 요청한 명시적 export에서만 계산하는 계약을
정의해야 한다. 계산하지 않은 증거를 빈 집합의 digest로 대신하지 않는다.

원하는 K개 해법만 원본 집합 의미를 보존해 추출하고, 실행 trace는 그 K개에 한정해
symbolic predecessor 또는 기존 verifier로 복원할 수 있다. 모든 해법의 모든 trace를
요청하면 그 출력 크기만큼의 비용은 피할 수 없다. family root는 arena 소유자와
수명을 함께 유지해야 하며 다른 worker/프로세스의 임의 ID와 섞지 않는다.

## 7. 연산량·메모리에서 실제로 달라지는 항

기존 비용은 대략 `Σ(C∈F) Build(C)+source(C)`다. 제안한 방식의 비용은
각 층의 **고유한 apply/cofactor/관계 subproblem 수와 압축된 결과 크기**에 의존한다.
raw complete cover 수 P를 반드시 한 번씩 방문하는 항은 없다.

독립된 영역 m개에 선택지가 c개씩 있고 물리·공급 조건도 작은 경계로 분해된다면,
P=c^m개의 완성 조합 대신 각 factor와 경계만 처리할 수 있다. 이것은 단순히
저장 위치를 바꾼 것이 아니라 같은 잔여 함수를 공유해 작업을 없애는 경우다.
실제 Full 6L에 그 독립성이 있다는 주장은 하지 않는다.

반대로 물리·공급 관계가 모든 부분 선택을 구별하면 압축은 약해진다. 작은 식
`∧_i(x_i↔y_i)`도 변수 순서를 모든 x 다음 모든 y로 두면 x를 읽은 위치에서
2^m개의 잔여 함수를 구별해야 한다. x_i,y_i를 교차 배치하면 작아질 수 있다.
이는 실제 Full 6L 폭증의 증명이 아니라 변수 순서와 관계 경계의 중요성을 보여 주는
일반 반례다.

따라서 계산량은 `O(원본 DAG nodes)`라고 약속하지 않는다. 각 단계의 unique
subproblem 수 U_k, live 관계 nodes Z_k와 중간 allocation을 기록해야 한다.
symbolic relational product 자체도 최악에는 지수적 비용을 가질 수 있다.
[NuSMV 공식 설명](https://nusmv.fbk.eu/papers/sttt_j/html/node24.html)

기존 기록의 52,325,237 nodes는 16바이트 payload만 837,203,792바이트다.
node마다 8바이트 표 하나를 추가해도 418,601,896바이트가 든다. 원본이 압축되어
있다는 이유로 여러 dense side table이나 worker별 전체 복제본을 더하면 안 된다.
다음 소유 구조를 우선한다.

- 원본 geometry arena는 immutable하게 공유하고 Down은 우선 가상 view로 사용한다.
- 매 rank의 현재/다음 root를 중심으로 살아 있는 관계를 유지한다. witness 보관은
  별도이며 모든 층과 모든 경로를 기본적으로 복사하지 않는다.
- 중간 unique/apply table까지 포함해 peak를 계산하고 마지막 사용이 끝난 참조를
  회수한다. root가 살아 있는데 arena를 재활용하지 않는다.
- 전체 허용 논리 CPU와 준비된 worker 즉시 투입 정책을 유지한다. 독립 relation
  partition의 작업을 병렬화하되 worker별 전체 family 복제는 피한다.
- NUMA는 immutable 입력 공유와 작업자 소유 scratch의 위치를 기준으로 다룬다.
  단일 NUMA에서 이득 없는 추가 shard·복사·동기화를 강제하지 않는다.

## 8. 구현 전후의 판정 기준

| 기준 | 필요한 확인 |
| --- | --- |
| 알고리즘 의미 | accepted family와 기존 physical/source 의미의 양방향 동치 |
| 완성 후보 전개 | 결과 page 추출 전 complete-candidate 생성·verifier 호출 0 |
| 숨은 전개 | 전처리·count·동치 인증에서 모든 assignment를 생성하지 않음 |
| 문맥 공유 | `(family node, semantic context)` 수와 최대 잔여 함수 폭 |
| 연산량 | apply miss·cofactor·행 frame 치환·source transition 수 |
| 메모리 | live nodes뿐 아니라 allocated nodes·unique/memo·scratch의 peak |
| Unique | 같은 row-set의 여러 순서·큐·Hold witness가 한 타일링으로 합쳐짐 |
| 출력 | 전체 identity 배열 없이 exact count·첫 page·대표 trace 가능 |
| 실패 | Unsupported·자원 거절·취소를 empty/complete 결과로 바꾸지 않음 |

`complete_candidate_visits=0` 하나만으로 성능 성공을 판정하지 않는다. 다른 이름의
거대한 상태표를 만들면 같은 문제가 남는다. 반대로 압축 family의 많은 원소를
암묵적으로 다룬다는 이유만으로 그것을 열거했다고 부르지도 않는다. 판단 대상은
실제로 생성한 개별 상태와 반복한 관계 subproblem이다.

첫 구현은 연구 경로로 완성하고, 기존 verifier는 제한된 correctness oracle 및
선택된 witness 복원에 사용한다. 연구 경로의 실패를 자동으로 전체 후보 열거로
바꾸어 완료 시간을 숨기지 않는다. 작은 입력 시간 벤치마크나 기존 100% 확률
재검증은 성공 기준이 아니다.

## 9. 다음 구현 순서

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_family.rs`, 구현 내용: 원본 DAG의 고정 rank·row-set 의미·인증된 Product 범위를 명시하고, 후보 열거 없이 Down을 해석하는 view와 중복 유도를 하나로 합치는 set 연산 경계를 정의한다. 기존 산술 path_count와 권한을 분리한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_partial_family.rs` (신규), 구현 내용: 전체 catalog row-ID의 symbolic 부분 선택과 L_k 관계를 소유하고, 완성 C를 carry하지 않는 Add·join·projection을 구현한다. concrete subset 전체 표를 만들지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_board_family.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs`, 구현 내용: S에서 원본 점유·삭제 행·실제 realization을 유도하고, 현재 domain에 필요한 ordered-kick 관계를 분할해 결합·소거한다. 기존 64변수 검산기를 단순 승격하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/coverage_product.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/piece_order_language.rs`, 구현 내용: source/Hold/EOF를 L_k와 정확히 결합하고, 후보 존재성과 원본별 coverage에 필요한 provenance를 구분한다. 큐별 또는 complete-candidate별 외곽 반복을 추가하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/result.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/parallel_worker.rs`, 구현 내용: accepted family의 수명·정확한 Unique count·page·대표 trace를 다루는 결과 경로를 추가하고, 전체 identity Vec/HashSet의 강제 materialization을 제거한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/performance/search_stage_profiler.rs`, `crates/clearra-core-executor/examples/full6l_bench.rs`, 구현 내용: 위 구조 판정 계수를 추가하고, 향후 승인된 유한 진단에서 원본 DAG와 중간 관계·첫 결과 출력까지의 비용을 함께 확인한다. CI 승인 전 실행·조회·대기를 시작하지 않는다.

## 10. 외부 연구와 Clearra 설계의 관계

| 원본 연구 | 이번 설계에 사용하는 원리 | 그대로 가정하지 않는 것 |
| --- | --- | --- |
| [Burch·Clarke·Long, 1991](https://www.cs.cmu.edu/~emc/papers/Conference%20Papers/Representing%20Circuits%20More%20Efficiently%20in%20Symbolic%20Model%20Checking.pdf) | 상태 집합의 relational image, 분할 전이, 마지막 사용 뒤 변수 소거 | 회로에서의 성능 수치가 Tetris에서도 재현된다는 보장 |
| [Darwiche·Marquis, 2002](https://www.cril.univ-artois.fr/~marquis/darwiche-marquis-jair02.pdf) | 표현 크기·count·결합·소거 지원을 각각 평가 | 작은 입력 DAG면 모든 후속 연산도 작다는 가정 |
| [Kawahara 외, 2014 technical report / 2017 논문](https://www-alg.ist.hokudai.ac.jp/~thomas/TCSTR/tcstr_14_76/tcstr_14_76.pdf) | 개별 해를 먼저 출력하지 않고 압축된 해 집합을 직접 구성·필터 | 기하 frontier만으로 물리·공급의 전체 문맥을 대체 |
| [Minato 연구 프로젝트](https://www.jst.go.jp/erato/minato/index_en.html) | Boolean 함수와 희소 조합 family의 집합 연산 | ZDD로 변환하면 항상 메모리와 시간이 감소한다는 가정 |

Down(F)와 ILC 상태 식을 연결한 위 recurrence 및 정확성 논증은 이번 Clearra 코드
계약으로부터 도출한 설계다. 외부 논문이 Clearra의 새 구현이나 Full 6L 성능을
검증했다고 표현하지 않는다.

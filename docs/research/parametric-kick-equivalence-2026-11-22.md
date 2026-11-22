# 순서 있는 임의 킥 테이블의 Boolean 배치 의존성 동치

작성일: 2026-11-22

## 1. 결론과 명제의 범위

고정 타일링의 Boolean 배치 가능성을 계산하는 핵심 관계는 SRS 계열의 특정 킥 목록에 의존하지 않는다. 원본 순서를 보존한 유한 킥 목록을 매개변수로 받아, 모든 점유 보드에서 원시 이동 해석기와 같은 관계를 계산할 수 있다. 필요한 것은 회전 후보의 순서, 정확한 충돌 조건, 완전한 목표 footprint, 진입 계약을 유지하는 것이다.

이 문서가 증명하는 범위는 다음과 같다.

| 대상 | 결론 |
| --- | --- |
| 표준 7종 미노, 정규화된 shape, 유한 순서 킥 목록 | 특정 프로필 ID와 무관한 전이 관계를 정의한다. |
| 모든 유효한 상공 자세를 seed로 하는 all-sky 진입 | 아래 정리 U에 의해 유한 높이 `H+D+U`가 정확하다. |
| 더 작은 `H+D` 경계 | 실제 순서 목록이 충분조건 A/B를 만족할 때만 사용한다. |
| source/destination 충돌, first-success, E2 간선 제거 | 모든 점유 보드 또는 명시된 cube에 대해 양방향으로 정확하다. |
| 여러 orientation으로 실현되는 같은 footprint | 모든 realization의 존재적 합집합을 사용해야 한다. |
| 줄 지우기 이후의 고정 타일링 | 원본 행 frame을 보존하는 투영과 결합하면 배치 수에 대한 귀납으로 정확하다. |
| 고정 spawn, lock delay, 실제 입력 횟수, finesse, 점수 증거 | 이 all-sky Boolean 정리의 결론에 포함하지 않는다. |

구현의 보드 어댑터는 현재 폭 1–10, 높이 1–6의 Board64 영역을 사용한다. 제품에서 자동 적용하는 PC 요청 범위는 별도의 admission 계약이다. 아래 높이 정리는 임의의 유한 `H`에 적용되며, 제품 어댑터의 지원 높이와 수학 정리의 범위를 혼동하지 않는다.

“기존 기준과 같다”는 표현에도 구분이 필요하다. 내장 5프로필은 검증된 `H+D` 축약과 일반 경계가 같은 low lock 관계를 준다. 과거 `H+D`를 임의의 custom 목록에도 적용하면 거짓 양성이 생기는 반례가 있다. 그 잘못된 유한 근사와 항상 같은 결과를 요구할 수는 없다. 범용 경로의 기준은 아래에 정의한 원시 순서 이동 의미론이며, 잘못된 custom 경계의 결과는 수정 대상이다.

## 2. 좌표, 보드 비트, 회전 원점

보드 셀 집합은

\[
\Omega=\{0,\ldots,W-1\}\times\{0,\ldots,H-1\}
\]

이고 점유 보드를 `B ⊆ Ω`로 둔다. 구현에서는 셀 `(x,y)`를 `yW+x`번째 비트로 표현한다.

미노 종류를 `p`, orientation을 `r∈{0,1,2,3}`, 정규화된 네 셀 집합을 `S[p,r]`로 둔다. 이 문서의 registry 계약은 각 shape의 최소 x와 최소 y가 0이고 모든 셀 좌표가 음수가 아니라는 것이다. `CompactKickAuthority`가 요청을 승인할 때 이 전제를 실제 registry에서 확인한다.

자세는 `q=(p,r,x,y)`이며 전체 물리 footprint는

\[
F(q)=\{(x+a,y+b):(a,b)\in S[p,r]\}
\]

이다. 벽·바닥 밖의 셀이 하나라도 있으면 그 자세는 기하학적으로 유효하지 않다. 유효한 자세의 보드 내 충돌 마스크는 `M(q)=F(q)∩Ω`다. 상공 셀은 충돌 마스크에서 빠지지만 물리 shape에서 없어지는 것은 아니다.

### 2.1 정규화 anchor와 rule 원점은 다르다

orientation마다 rule 좌표의 원점을 표현하는 보정량을 `c[p,r]`라 둔다. 정규화 anchor가 `a`일 때 rule 원점은 `a+c[p,r]`이고, 원본 킥 `k`를 적용한 다음 anchor는

\[
a'=a+k+c[p,r]-c[p,s].
\]

따라서 정규화된 킥은

\[
\delta_{p,r\to s,j}=k_{p,r\to s,j}+c[p,r]-c[p,s].
\tag{N}
\]

[rule의 원점 변환](../../crates/clearra-rules/src/kicks/kick_table.rs)을 사용하는 [wide 정규화](../../crates/clearra-core-executor/src/backend/wasm_cpu/kick_profiles.rs)는 사용자 오프셋을 더하기 전에 `i16`으로 넓힌다. 원본 `i8`의 −128과 127에서도 덧셈이 wrap되지 않는다. 직접 독립 해석기는 원점 보정 표와 원본 셀 좌표로 이 산식을 다시 계산한다.

닫힌 orientation 열에서는 보정항이 망원합으로 상쇄되므로 전체 anchor 이동은 원본 킥의 합이다. 그러나 **한 번의** raw zero-kick에서 anchor 이동이 0일 필요는 없다. 특히 I의 원점 표를 별도 증명 없이 단순 유클리드 회전축이라고 부르지 않는다. 여기서는 엔진이 사용하는 rule 좌표 변환을 정의로 취급하며, 외부 게임의 물리 구현과 같다는 주장은 별도 검증 대상이다.

O는 현재 registry의 네 footprint가 같고 보정항도 모두 0이다. 그래도 custom O 목록에 위치 이동 킥이 있으면 O가 움직이지 않는다고 가정해서는 안 된다. O 전용 단조성 가지치기는 **실제 활성 목록의 모든 정규화 이동이 0**이라는 별도 capability가 있을 때만 성립한다.

## 3. 원시 전이의 정확한 Boolean 식

\[
\operatorname{Free}_B(q)=
\operatorname{ValidGeometry}(q)\land(B\cap M(q)=\varnothing).
\]

좌우·아래 이동 `q→t`의 전이는

\[
E_B(q,t)=\operatorname{Free}_B(q)\land\operatorname{Free}_B(t)
\tag{T}
\]

이다. 위쪽 한 칸 이동은 이 전이 집합에 없다. 상향 이동은 명시된 회전 후보에 의해서만 생긴다.

한 회전 요청의 원본 순서 목록을 `t₀,…,tₘ₋₁`이라 하자. 후보 `j`로 가는 전이는

\[
E_{B,j}(q,t_j)=
\operatorname{Free}_B(q)
\land\operatorname{Free}_B(t_j)
\land\bigwedge_{\ell<j}\neg\operatorname{Free}_B(t_\ell).
\tag{R}
\]

기하학적으로 무효인 후보는 `Free=false`이므로 packed 목록에서 생략해도 된다. 하지만 유효한 앞선 후보는 자기 간선이 불가능하다는 이유로 뒤 후보의 guard에서 생략할 수 없다. 이미 방문한 후보도 `Free=true`이면 first-success를 소비한다.

이 식은 점유량에 대해 단조롭지 않다. 블록을 추가하여 앞선 후보를 막으면 뒤 후보가 새로 선택될 수 있다. 따라서 단순한 최소·최대 보드 두 개의 도달성만으로 중간 보드의 답을 확정하지 않는다.

구현 연결:

- [template compiler와 reverse guard](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability.rs)는 전체 forward 후보 목록과 원래 raw ordinal을 보존한다.
- [forward lane 관계](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_prefix.rs)는 각 lane에서 처음 성공한 후보가 그 lane을 소비한다.
- [reverse target 관계](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_target.rs)는 원래 packed prefix 전체의 `Free`를 부정하여 식 R을 계산한다.

## 4. 정리 U: 임의 순서 킥의 상공을 유한하게 줄일 수 있다

해당 미노의 모든 활성 정규화 킥 목록에서

\[
D=\max(0,\max(-\delta_y)),\qquad
U=\max(0,\max\delta_y),\qquad
C=H+D+U
\]

로 둔다. 목록은 유한하므로 D와 U도 유한하다.

**무한 모델**은 anchor `y≥0`인 모든 기하학적 유효 자세를 포함하고 `y≥H`인 모든 자세가 seed다. **유한 모델**은 anchor `0≤y≤C`만 포함하고 이 중 `y≥H`인 모든 유효 자세가 seed다. 둘 다 식 T/R과 같은 보드를 사용한다.

**정리 U.** 모든 `B⊆Ω`와 모든 `y<H`인 목표 자세에 대해, 무한 모델과 유한 모델의 도달성은 같다. 목표의 grounding을 추가한 Boolean lock 가능성도 같다.

### 증명 U1: low source의 전이를 모두 보존한다

source의 anchor가 `y≤H−1`이면 모든 회전 후보의 anchor는

\[
y+\delta_y\le H-1+U\le C
\]

이다. 따라서 어떤 앞선 후보도 위 경계 때문에 빠지지 않는다. 벽·바닥 검사와 보드 충돌은 같은 셀을 읽는다. 좌우·아래 이동도 위 경계를 넘지 않는다. 그러므로 두 모델의 low source에서 식 T/R은 동일하다.

### 증명 U2: 마지막 sky→low 전이를 보존한다

아래 한 칸 이동으로 sky에서 low로 들어오면 source의 anchor는 정확히 H다. 이 source와 목표가 모두 유한 모델에 있다.

회전 후보 `j`가 sky source `yₛ≥H`에서 low에 들어오면

\[
y_s+\delta_{j,y}\le H-1
\quad\Rightarrow\quad
y_s\le H-1+D.
\]

이 source에서 **모든 후보**, 특히 j보다 앞선 후보의 anchor는

\[
y_s+\delta_{\ell,y}\le H-1+D+U=C-1
\]

이다. 따라서 candidate 순서 판정을 바꾸는 위쪽 clipping이 없다. source 및 목표의 유효성, 모든 앞선 후보의 충돌값도 같다. sky→low 전이가 양방향으로 보존된다.

### 증명 U3: 경로 전체를 대응시킨다

어느 모델에서든 low 목표까지 가는 유한 witness 경로를 잡고 **마지막 sky 방문 이후의 suffix**만 취한다. 마지막 sky source는 U2의 범위 안에 있으며 양쪽 모델의 seed다. 첫 sky→low 간선은 U2, 그 이후의 모든 low source 간선은 U1에 의해 보존된다. 이를 경로 길이에 대해 귀납하면 상대 모델에서도 같은 suffix가 존재한다.

이 논증은 양쪽 방향으로 적용된다. 높은 sky 안에서 잘린 간선 때문에 다른 sky 경로가 생기거나 사라져도, 모든 sky 자세 자체가 seed인 계약에서는 마지막 sky source의 도달성을 다시 입증할 필요가 없다. 전체 상공 그래프의 bisimulation을 주장하는 것이 아니라 **low 목표까지의 Boolean 도달성 동치**를 증명한 것이다.

grounding은 바닥 또는 한 칸 아래 footprint의 점유만 검사하므로 두 모델에서 같다. 따라서 lock 가능성도 같다. □

### 이 증명이 적용되지 않는 진입 계약

하나의 고정 spawn에서 시작한다면 마지막 sky source가 실제로 도달 가능한지 다시 보존해야 한다. 모든 sky를 seed로 바꾸는 U3를 사용할 수 없다. 별도 entry/window API는 유한 경계를 선언한 계약을 유지하며, 이 문서는 해당 API를 무한 fixed-spawn 게임의 완전한 모델로 승격하지 않는다.

## 5. 더 작은 경계는 프로필 이름이 아니라 검증된 capability다

기존 내장 테이블은 더 작은 `C₀=H+D`를 쓸 수 있다. 이를 일반화 경로에 그대로 적용하지 않고 다음 충분조건으로 분리한다.

**A.** `U≤D`.

**B.** 각 순서 목록, 후보 j, 정수 `0≤t≤D`에서 `t+δⱼ,y<0`이고 앞선 후보 중 `t+δₖ,y>D`인 것이 있다면, j보다 앞에 다음을 만족하는 h가 있다.

\[
\delta_{h,x}=\delta_{j,x},\qquad
0\le t+\delta_{h,y}\le D.
\]

A는 low source의 모든 후보가 `C₀` 안에 남음을 보장한다. B에서 sky source를 `H+t`로 보면 h는 보드 위의 빈 상공에 있고, 목표 후보 j와 같은 x 및 같은 목적 orientation을 사용한다. j가 벽 안에 들어가면 h도 벽 안에 들어간다. 따라서 h는 반드시 먼저 성공한다. 잘린 높은 후보 때문에 원래 불가능하던 j가 유한 모델에서 활성화될 수 없다. 나머지는 정리 U의 마지막 sky suffix 논증과 같다. □

`KickSkyBounds`는 실제 정규화 목록을 검사하고 private `ReducedSkyCeiling` token을 생성한다. token이 없으면 범용 `H+D+U`를 사용한다. 프로필 ID가 SRS라고 해서 token을 승계하지 않는다. 내장 5프로필 × 7미노에 대해 실제 목록이 A/B를 만족하는지 검사하는 회귀도 유지한다.

검사 자체도 매 후보마다 앞선 목록을 재탐색하지 않는다. A 아래에서 앞선 `dy≥0`은 source 높이 매개변수 `t∈[0,D-dy]`를, `dy≤0`은 `t∈[-dy,D]`를 덮는다. 같은 x의 이전 후보들이 덮는 집합은 양끝 두 구간의 합집합이므로 가운데 미포함 구간만 저장하면 된다. 이전 최대 dy와 이 두 구간을 prefix마다 갱신하여 B를 검사한다. 후보 수 m, 서로 다른 x 수 K에서 `O(m log K)`이며, 긴 목록에 대한 이차 prefix 재검사를 피한다. 별도 유한 검산은 이 구간 판정과 B의 원래 전칭식을 직접 비교한다.

O의 위치 불변성, no-kick의 특수 단조성 등은 이 범용 전이 정리와 별개다. 기존 [O capability 검사](../../crates/clearra-core-executor/src/backend/wasm_cpu/monotone_reachability.rs)처럼 실제 shape와 목록을 확인한 뒤 사용할 수 있다. raw zero-kick이라는 이유만으로 다른 미노의 정규화 anchor가 움직이지 않는다고 일반화하지 않는다.

## 6. 정리 E2: cube에서 절대로 생기지 않는 간선

서로 겹치지 않는 fixed occupied 집합 F와 variable 집합 V를 두고

\[
\mathcal C(F,V)=\{F\cup X:X\subseteq V\}
\]

로 둔다. 나머지 셀은 비어 있다. source와 목표가 기하학적으로 유효하다고 하고 `E=M(q)∪M(tⱼ)`라 하자.

**정리 E2.** 식 R의 간선이 cube의 어떤 보드에서 존재할 필요충분조건은

\[
F\cap E=\varnothing
\quad\land\quad
\forall\ell<j:\;\operatorname{ValidGeometry}(t_\ell)\Rightarrow
\bigl[(F\cap M(t_\ell)\ne\varnothing)
\;\lor\;
(M(t_\ell)\cap V\setminus E\ne\varnothing)\bigr].
\tag{E2}
\]

**필요성.** source와 목표는 비어야 하므로 F가 E를 막을 수 없다. 앞선 각 유효 후보에는 blocker가 필요하다. fixed blocker가 없으면 variable 셀 중 E 밖의 셀이 blocker여야 한다.

**충분성.** 단일 보드

\[
B^*=F\cup(V\setminus E)
\]

를 고르면 source/목표는 비어 있고 모든 앞선 후보가 동시에 막힌다. 각 후보마다 서로 다른 보드를 택할 필요가 없다. □

translation은 앞선 후보 조건이 없는 동일한 식이다. source나 목표의 기하가 무효이면 간선은 즉시 불가능하다.

현재 production 역방향 compiler의 전역 제거는 `F=∅, V=Ω`인 특수 경우다. 따라서 어떤 board에서도 쓸 수 없는 간선만 제거한다. 원본 forward 후보와 제거된 후보의 `Free` 읽기는 모두 유지한다. 이 정리는 킥의 크기·종류·순서 길이에 대한 SRS 가정을 사용하지 않는다.

상관관계가 있는 실제 후보 보드 가족 `Γ⊆C(F,V)`에도 E2의 불가능 판정은 안전하다. 그러나 E2의 양성 witness `B*`가 Γ에 속한다는 보장은 없으므로, cube 양성을 실제 도달성이나 실제 타일링 가능성으로 승격해서는 안 된다.

## 7. 보드 집합의 도달성과 조합론적 귀납

유한 template의 자세 집합을 Q, seed 집합을 S라 하자. 각 보드의 도달 집합은

\[
R_B^0=\{s\in S:\operatorname{Free}_B(s)\},
\qquad
R_B^{k+1}=R_B^k\cup\{t:\exists q\in R_B^k,E_B(q,t)\}.
\]

경로 길이 k에 대한 귀납으로 `q∈R_B^k`와 “seed에서 길이 k 이하의 실제 전이 경로가 존재한다”가 동치다. 유한 Q에서 최소 고정점에 도달하면 모든 길이의 경로를 포함한다. SCC나 회전 순환이 있어도 이 귀납은 바뀌지 않는다.

각 자세에 대해 보드 가족

\[
\mathcal R_q=\{B:q\in R_B\}
\]

를 저장하면 같은 재귀를 집합 합집합·교집합·여집합으로 계산할 수 있다. BDD든 lane 비트셋이든 집합 연산을 정확하게 구현하면 각 원소 B에 대한 위 귀납이 동시에 성립한다. 정방향 lane 계산은 `Free` 집합과 first-success 차집합을 사용하며, 역방향 target 계산은 동일 간선의 정확한 역관계를 사용한다.

작업량에 따른 경험적 중단을 음성으로 처리하면 안 된다. lane 실행에서는 한 `(pose,lane)`이 최초 방문될 때만 새로운 pending bit를 추가하므로, queue pop 수는 최대 `|Q|×활성 lane 수`다. 중복 경로가 많아도 새 방문 bit 수는 유한하다. 이번 일반화는 1,024 poses 같은 작은 경험적 허용량을 이 표현 범위와 종료 불변식으로 바꾸었다. 취소·메모리 거부는 별도의 실패 결과이며 “도달 불가”가 아니다.

## 8. 목표 footprint, grounding, 여러 realization

충돌 마스크는 sky 셀을 생략할 수 있으므로 3셀짜리 clipped mask가 4셀짜리 lock을 대신할 수 없다. Boolean target API는 목표가 정확히 4셀이고 template의 해당 자세 마스크가 주어진 `lock_mask`와 완전히 일치하는지 확인한다.

고정 operation i의 현재 row frame에서 가능한 realization 집합을 `I(i,D)`라 하면

\[
\operatorname{CanPlace}_B(i,D)=
\bigvee_{q\in I(i,D)}
\bigl(q\in R_B\land\operatorname{Ground}_B(q)\bigr).
\tag{P}
\]

목표 아래 한 칸이 바닥 밖이거나 점유와 겹쳐야 grounded다. 다른 미노가 모두 없어도 바닥에 닿는 경우는 외부 선행 미노를 요구하지 않는다. 공중 목표는 아래 footprint를 막는 보드 셀이 필요하다는 조건을 얻는다.

같은 전체 footprint를 Zero와 Two 등 여러 자세로 구현할 수 있어도 한 자세만 실패했다고 operation 전체를 실패시킬 수 없다. 역방향 target lane 계산은 모든 realization을 seed로 넣고 합집합을 계산한다. 양성 representative를 필요로 하는 경로는 실제로 도달 가능한 realization을 다시 선택해야 하며, catalog의 첫 orientation을 무조건 반환해서는 안 된다.

## 9. 줄 지우기와 고정 타일링에 대한 귀납

원본 행 중 이미 삭제된 집합을 D라 하자. 삭제되지 않은 행 y의 현재 행 번호는

\[
\pi_D(y)=y-|\{d\in D:d<y\}|.
\]

이 함수는 살아 있는 행들의 순서 보존 전단사다. 따라서 삭제되지 않은 원본 셀끼리의 겹침 여부는 같은 frame에서 투영한 셀의 겹침 여부와 같다. 이미 삭제된 행을 목표가 차지하면 그 realization은 불가능하다.

[catalog의 `instantiate_realization_raw`](../../crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs)는 삭제 전제, 목표와 삭제 행의 불일치, 현재 네 셀의 기하 유효성, 현재 셀을 원본 행으로 되돌렸을 때의 **전체 target equality**를 검사한다. anchor의 높이만 맞는 것으로 대체하지 않는다.

0회 배치에서 실제 보드와 투영 보드가 같은 것을 기저로 둔다. k회까지 같고 원본 row frame D도 같다고 가정하자. 식 P는 다음 operation의 실제 합법성과 동치다. 합법한 footprint를 OR하고 같은 완성 행을 지우면 두 모델의 다음 보드 및 frame도 같다. 따라서 k+1회 상태가 같다. 배치 수에 대한 귀납으로 모든 operation 순서의 수용 여부가 같다.

같은 미노 종류가 반복되더라도 operation은 고정 타일링의 개별 footprint identity로 먼저 구별한다. 종류·공급에 따른 quotient는 이 정확한 operation 관계 위에서만 적용한다. 동일 compact board라도 원본 삭제 frame이 다르면 남은 operation의 realization 집합이 달라질 수 있으므로, board 비트만으로 frame을 합치지 않는다.

공급·hold·관측 정책의 quotient와 확률 집계는 별도 [관측 quotient 증명](observation-quotient-proof-2026-11-22.md)에 연결한다. 이 문서는 그 증명의 전제인 정확한 물리 action 관계를 제공한다. 단순 미노 DAG만으로 ordered kick의 부정 조건이나 줄 지우기 조건을 잃어도 된다고 주장하지 않는다.

## 10. 구현 계약과 자원 거부

[CompactKickAuthority](../../crates/clearra-core-executor/src/backend/wasm_cpu/compact_kick_authority.rs)는 다음을 소유한다.

- 실제 검증된 테이블에서 활성 CW/CCW/180 요청의 전체 정규화 순서 목록, 누락과 명시적 빈 목록의 구별.
- 요청한 rule, profile metadata, 폭·높이, 180도 요청 활성 여부.
- 실제 목록으로 증명한 미노별 상공 경계.

동일 profile ID가 같은 순서 목록이라는 뜻은 아니다. 공유 template와 private workspace는 같은 exact authority를 바인딩해야 한다. builtin ID만 보는 선택적 증거는 supplied authority의 내용에 대해 별도 검증되지 않았으면 재사용하지 않는다.

검증된 입력이 추가 `from==to` 레코드를 갖는 경우는 활성 요청으로의 투영에서 제외한다. 원시 해석기에도 같은 orientation을 목적지로 요청하는 네 번째 회전 버튼은 없으므로, 이 레코드는 어떤 source에서도 조회되지 않는다. 그 레코드를 넣거나 빼도 식 T/R, seed 및 목표 조건이 모두 같고, 따라서 위 귀납에 의해 모든 보드의 결과가 같다. authority equality는 이 정확한 **활성 목록 투영**을 비교한다. 전체 entry 수를 84개로 잘라 거부하지 않으며, 실제 84개 활성 slot의 유일성을 확인한다. 이 처리는 의미가 있는 후보의 순서 변경이나 삭제와 다르다.

범용 정리는 명시된 shape·원점·진입 계약을 가진 전이 시스템에 대한 것이다. `VerifiedKickTableProfile`은 transition의 완전성·중복·unsupported annotation을 검증하지만 별도의 게임 회전축이나 spawn 계약까지 증명하지 않는다. 현재 binding은 다음과 같이 구분한다.

| source rule 계약 | supplied table binding |
| --- | --- |
| Srs, SrsPlus, SrsX, Jstris180, NoKick | 표준 정규화 shape 및 all-sky 계약으로 전체 순서 목록을 허용한다. profile ID는 Imported/Custom이어도 된다. |
| Asc, Ars | 기존 capability가 별도의 spawn reachability를 요구하므로 명시적 geometry/entry 계약 없이 이 경로에 넣지 않는다. |
| Custom rule | 원점·shape·entry 계약이 따로 제공되지 않아 명시적으로 거부한다. Custom **table ID**와 다르다. |

다른 회전축의 움직임도 식 N에 맞춰 오프셋을 보정하여 표준 원점 계약의 목록으로 표현할 수 있으면 같은 정리가 적용된다. 단지 source rule의 이름만 보고 그러한 변환이 이미 되었다고 추측하지 않는다.

내부 anchor y와 정규화 delta는 `i16`이다. 입력 raw offset은 원래의 전체 `i8` 범위를 그대로 받는다. raw ordinal과 packed earlier-count는 `u32`이며 255개 뒤의 후보도 순서·guard를 보존한다. 공개 below-board lock 좌표와 기존 explicit-entry 기록의 `i8` 계약은 변경하지 않는다.

입력 parser가 작은 후보 개수 제한을 보장한다고 가정하지 않는다. compiler는 유한 메모리와 인덱스 표현 때문에 logical pose·target·offset 수에 대해 checked resource admission을 수행한다. 현재 기본 compiler admission의 총 상한은 100,000 poses, 64,000,000 potential rotation targets, 600,000 index entries이며, 개별 template의 u16 state sentinel 및 u32 offset 범위도 확인한다. 이는 자원 거부이며 목록을 잘라서 계산하겠다는 의미가 아니다. 이런 유한 실행 자원 제한은 임의 길이 유한 목록에 대한 수학 정리 자체의 제한과 구분한다.

승인 순서에도 메모리 계약을 적용한다. 첫 단계는 고정 크기의 84개 활성 entry 참조만 저장하고, `C≥H`로 얻는 최소 pose·target·index 수를 목록 길이만으로 검사한다. 이어서 원본 목록을 iterator로 읽어 실제 상공 경계와 모든 aggregate count를 검증한다. 이 단계는 후보 수에 비례하는 정규화 복사본을 만들지 않는다. 충분조건 검사의 임시 구간 표는 한 전이의 서로 다른 raw x 값에 의해 유한하게 제한된다. 검증을 통과한 borrowed `AdmittedKickLayout`만 둘째 단계로 이동하며, 그 단계에서 `try_reserve_exact`로 정규화 저장소를 할당한다. 거부될 긴 목록을 먼저 복사한 뒤 예산 초과를 알아내는 순서를 허용하지 않는다.

메모리 projection도 nominal ID가 아니라 해당 owner의 실제 경계와 목록 길이로 계산한다. forward offset `3n+1`, reverse offset `n+1`, 동시에 살아 있는 reverse cursor `n+1`을 모두 포함하고, 넓어진 ordinal·reverse entry 및 lane scratch를 반영한다. 실제 allocator RSS와 시스템 자원 통제는 별도의 실행 계층 계약이다.

## 11. 반례와 실행 검증의 역할

다음은 증명 전제를 구현이 지키는지 확인하는 회귀들이다. 테스트 성공을 “모든 보드의 열거”나 귀납 증명 자체라고 부르지 않는다.

| 검증 | 겨냥하는 오류 |
| --- | --- |
| `board_family_custom_upward_kick_exposes_an_unqualified_ceiling` | O의 `(0,+7)` 앞선 후보를 `H+D`가 잘라 뒤의 `(+4,0)`을 잘못 허용하는 기존 반례. |
| `custom_upward_kicks_preserve_earlier_guards_beyond_narrow_sky_coordinates` | 자동 범용 경계에서 위 반례를 닫고 +127, 1,024 초과 pose, E2 off/on을 검사. |
| `complete_ordered_prefix_beyond_255_candidates_is_not_truncated_or_renumbered` | 300개의 앞선 후보 뒤에서만 선택되는 T 킥의 원본 ordinal·guard 보존. `0x4002`와 `0x4802`의 반대 답을 검사. |
| `arbitrary_raw_offset_endpoints_and_empty_sequences_match_independent_geometry` | 모든 7미노에서 raw −128/127 정규화, 높은 sky, 빈 transition의 실제 scalar lock 집합 동치. |
| `inactive_self_transition_metadata_preserves_the_exact_action_owner` | 84개를 넘는 입력 레코드에서 실제 요청하지 않는 self transition만 제거한 투영 동치. |
| `raw_lengths_and_expanded_sky_are_admitted_before_normalized_storage_exists` | 목록 단독 길이 제한을 통과해도 aggregate target 수나 상공 확장 예산을 넘으면 정규화 저장소 생성 전에 거부. |
| shared owner 및 실제 serial/parallel consumer 검사 | 같은 ID의 다른 순서 목록을 섞지 않고, supplied Imported 표를 원시 순차 배치·줄 지우기와 같은 전체 큐 비트맵으로 연결. |
| 기존 builtin 구조 검사와 5L/6L lane 검사 | 기하, 역간선, first-success, 모든 realization의 기존 계약 보존. |

독립 all-sky 해석기는 production capability 검사를 호출하지 않고 raw 원점·shape·순서 목록에서 `H+D+U`를 직접 계산한다. 따라서 builtin compiler의 더 작은 경계와 같은 ceiling을 복사한 비교에 머무르지 않는다. 별도 explicit-entry/window 해석기는 자신의 기존 유한 범위를 유지한다.

실행 결과와 명령·자원 내역은 이번 작업의 중앙 검증 기록에서 관리한다. 이 문서의 정리 U, E2 및 두 귀납 증명은 특정 테스트 성공 건수를 전제로 하지 않는다.

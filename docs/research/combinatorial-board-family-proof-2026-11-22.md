# 보드 집합의 조합론으로 구성하는 고정 타일링 의존성

작성일: 2026-11-22 KST

제품 소스 기준: `f1c57e4eab23734fcc6723c865dc50a1490b4d54`.
선행 수학 문서 기준: `61f08a8dea571f6734c66eabd0c32e84a8d94a61`.

## 0. 결론과 정확한 범위

**의존성 조건을 이미 알고 있다고 가정하지 않고, 보드 비트·형상·회전 원점·순서 있는 킥에서 조건을 구성한 다음 그 정확성을 귀납적으로 증명할 수 있다.** 이 문서는 그 구성과 증명, 실행 가능한 Rust 검산을 함께 기록한다. [이전 문서](fixed-tiling-ilc-dependency-proofs-2026-11-22.md)의 완전한 operation 명제 `e_i(S)`를 실제 보드 셀의 Boolean 식으로 만드는 부분을 구체화한다.

여기서 도달 범위는 단순한 셀 합집합이 아니다. **각 자세에 도달할 수 있는 보드들의 집합**이다. 이를 보존해야 서로 다른 보드에서만 가능한 이동을 잘못 연결하지 않는다. 전체 보드나 `2^n` operation prefix를 먼저 열거하지 않아도 이 집합을 Boolean 식으로 구성할 수 있다.

대상은 ILC 고정 타일링의 Boolean 배치/구축/PC 존재성이다. 공급과 관측을 결합할 때는 기존 oracle·see-7 정책 계약을 사용한다. 입력 이력, finesse 비용, spin 판정, B2B·damage·CountAll 경로 수는 이 증명의 상태에 포함하지 않는다.

| 항목 | 이번 상태 |
| --- | --- |
| 보드 family, ordered kick, 경로 길이 귀납, Shannon 귀납 | 아래 가정과 모형에 대해 증명 완료 |
| 도달성 계산 전 cube 간선 제거, 정적 영향 범위 축소 | 필요충분조건/안전성 증명 및 Rust 검산 완료 |
| 공동 변수 제거, 줄 삭제 후 Boolean 치환 | 정확한 조건 증명 및 Rust 검산 완료 |
| 내장 5프로필의 all-sky 유한 천장 | 모든 유한 높이에 대한 동치 증명 및 프로필 조건 검산 완료 |
| 1~6L의 구체 구현 대조 | 유한 입력 범위에 대한 검산 완료. 모든 보드 전수검사로 확대하지 않음 |
| 제품 경로 활성화·큰 P7P4 속도·메모리 개선 | 이번 변경에서 측정하거나 승인하지 않음 |
| 형식 증명기 검증 | Lean/Coq 등의 기계 검증은 수행하지 않음 |

새 분석 경로와 Boolean family 커널은 `cfg(test)`다. 기존 전역 영향 mask는 같은 내부 연산을 사용하도록 정리했으며, 새 cube 필터 없이 이전 의미를 유지한다. 기존 원본 작업 디렉터리의 미커밋 변경은 이 작업에 포함하지 않았다.

## 1. 모형: 보드 비트와 자세 집합

폭 `W`, 높이 `H`의 유한 셀 집합을 `C`, 셀 수를 `m=WH`라 한다. 모든 보드의 집합은

\[
\Omega=\{0,1\}^{C}\cong\mathcal P(C).
\]

보드 `B`의 셀 비트가 1이면 점유다. 서로 겹치지 않는 고정 점유 `O`와 변동 셀 `V`를 정하면

\[
\mathcal B(O,V)=\{O\cup X:X\subseteq V\}
\]

는 하나의 cube다. 이 밖 셀은 고정 빈칸이다. ILC·줄 삭제·공급 등으로 상관된 실제 허용 집합은 일반적인 `Γ ⊆ Ω`로 두며 cube와 혼동하지 않는다.

한 미노 종류에 대해 자세 `q=(r,a)`는 방향 `r`과 normalized anchor `a`로 정한다. 방향별 네 셀 형상을 `A_r`라 하면 전체 footprint는 `a+A_r`다. 벽·바닥 경계를 먼저 검사하고, 보드 높이 위의 빈 sky를 제외한 충돌 mask를 `M_q`라 한다. 정적으로 불가능한 자세와 mask가 비어 있는 유효한 sky 자세는 서로 다르다.

자세가 충돌하지 않는 보드 family는

\[
\mathcal F_q=\{B\in\Omega:\operatorname{Valid}(q)\land B\cap M_q=\varnothing\}.
\]

미노 움직임은 down/left/right와 프로필이 허용한 회전 입력이다. 회전은 ordered first-success 규칙을 따른다. 별도의 swept-volume 검사, 타이머, 이전 입력 상태를 요구하는 다른 규칙은 해당 상태와 guard를 추가해야 한다.

목표 배치는 **전체 네 셀 footprint와 종류**로 결박한다. sky에서 잘린 충돌 mask만으로 고정 타일링의 목표를 식별하지 않는다. grounded 조건 역시 도달성과 별개다.

## 2. 회전축을 보드 좌표로 옮기는 항등식

방향 `r`의 anchor에서 rule-coordinate 원점까지의 offset을 `c_r`라 한다. 원점 `p=a+c_r`에 raw kick `k`를 적용하고 목표 방향 `s`의 anchor로 되돌리면

\[
p'=p+k,\qquad a'=p'-c_s,
\]

따라서

\[
\boxed{a'=a+k+c_r-c_s.}
\tag{P1}
\]

이는 좌표계 변경의 항등식이다. 현재 [normalized_kick_delta](../../crates/clearra-rules/src/kicks/kick_table.rs)가 이 계산을 소유하므로 새 이동 엔진이나 원점 표를 제품에 중복 작성하지 않는다.

소스의 방향 순서 `Zero, Right, Two, Left`에 대한 `c_r`는 다음과 같다.

| 종류 | `c_r` |
| --- | --- |
| J/L/S/T/Z | `(1,0), (0,1), (1,1), (1,1)` |
| I | `(0,0), (-2,2), (0,1), (-1,2)` |
| O | 모든 방향 `(0,0)` |

특히 I의 `c_r` 자체를 normalized shape의 Euclidean 중심이라고 부르면 부정확하다. 현재 형상의 실제 local pivot은 `p_r=c_r+t`로 표현할 수 있으며 `t_I=(3/2,-1/2)`, `t_O=(1/2,1/2)`, `t_JLSTZ=(0,0)`이다. CW 회전 `R(x,y)=(y,-x)`에 대해 현재 네 셀 형상은

\[
A_{r+1}+c_r-c_{r+1}=p_r+R(A_r-p_r)
\]

를 만족한다. 반정수는 모든 좌표를 두 배로 하면 정수 집합 항등식으로 다룰 수 있다. 공통 offset은 `c_r-c_s`에서 소거된다.

연속 회전에서는

\[
a_n-a_0=c_{r_0}-c_{r_n}+\sum_j k_j.
\]

한 바퀴 돌면 원점 차가 상쇄된다. **중간 자세의 충돌 조건까지 상쇄되는 것은 아니다.** O의 모든 형상과 원점이 같고 프로필의 raw kick이 모두 0일 때만 회전이 점유 위치를 바꾸지 않는다. 다른 미노는 P1과 실제 ordered offset 목록으로 같은 집합 계산을 한다.

## 3. Ordered kick을 정확한 Boolean 조건으로 표현

평행이동 `u→v`의 활성 family는 `F_u ∩ F_v`다. 한 회전 입력의 순서 있는 후보가 `v_0,…,v_k`일 때 j번째 후보가 선택되는 family는

\[
\boxed{
\mathcal E_{u,j}=\mathcal F_u\cap\mathcal F_{v_j}
\cap\bigcap_{\ell<j}(\Omega\setminus\mathcal F_{v_\ell}).
}
\tag{E1}
\]

source와 선택된 target은 비어 있어야 하고, 앞 후보는 전부 실패해야 한다는 규칙의 직접적인 필요충분조건이다. 정적으로 불가능한 앞 후보의 `F`는 공집합이므로 blocker 항은 참이다. 유효한 앞 후보의 실패 조건은 `B∩M_v ≠ ∅`, 즉 그 mask의 셀 중 하나 이상 점유됨이다.

### 3.1 도달성 전에 간선을 없애는 조합론 정리

cube `B(O,V)`와 후보 j를 고정하고 `E=M_u∪M_{v_j}`라 하자. source와 target이 정적으로 유효하다는 전제 아래, 이 간선이 cube의 어떤 보드에서라도 활성일 **필요충분조건**은

\[
O\cap E=\varnothing
\quad\land\quad
\forall\ell<j:\quad
\operatorname{Valid}(v_\ell)\Longrightarrow
\left(
O\cap M_{v_\ell}\ne\varnothing
\;\lor\;
(M_{v_\ell}\cap V)\setminus E\ne\varnothing
\right).
\tag{E2}
\]

**필요성.** source/target이 요구하는 빈칸 `E`에 고정 점유가 있으면 불가능하다. 앞 후보를 막는 셀은 이미 `O`에 있거나, `E` 밖에서 점유시킬 수 있는 `V`의 셀이어야 한다.

**충분성.** 단 하나의 보드 `B*=O∪(V\E)`를 선택한다. `E`는 비고, 모든 앞 후보는 E2가 보장한 셀에 막힌다. 모든 blocker가 양성 점유 조건이므로 이들을 동시에 만족시켜도 서로 충돌하지 않는다. 따라서 j가 first success다. □

정적으로 불가능한 앞 후보는 이미 실패했으므로 E2의 blocker 의무에서 제외한다. Full cube에서는 **정적으로 유효한** 앞 후보 하나라도

\[
M_{v_\ell}\subseteq M_u\cup M_{v_j}
\]

이면 later edge는 영구적으로 불가능하다. source와 target이 자유로운 순간 그 앞 후보도 자유롭기 때문이다. 이는 실제 보드의 도달성이나 배치 순서를 먼저 탐색하지 않고 **유한 mask 연산만으로** 제거할 수 있는 간선이다.

`Γ ⊆ B(O,V)`인 상관된 domain에서는 E2 실패가 안전한 음성 증거다. E2 통과는 `Γ` 안에 witness가 있다는 뜻이 아니다. cube에서 얻은 필터는 같은 cube 또는 부분 cube에 재사용할 수 있고, cube를 넓히면 재인증해야 한다.

### 3.2 Blocker 흡수와 보존해야 할 읽기

고정 점유가 이미 막은 앞 후보는 clause에서 제거할 수 있다. 나머지를 `H_ℓ=(M_{v_ℓ}∩V)\E`로 쓰면

\[
H_a\subseteq H_b
\Longrightarrow
\operatorname{hit}(H_a)\land\operatorname{hit}(H_b)
=\operatorname{hit}(H_a).
\tag{E3}
\]

E3는 큰 clause를 흡수하는 정확한 식 축약이다. 모든 hitting set을 DNF로 전개하는 방식은 피한다. 서로 겹치지 않는 2셀 blocker k개만으로도 최소 witness 조합이 `2^k`개일 수 있다.

앞 후보 자신의 **간선**을 제거해도 그 후보의 **충돌 읽기**는 살아 있는 later edge에 남겨야 한다. predecessor cone 밖의 앞 후보도 먼저 성공하면 cone 안의 later 후보를 억제한다. 새 Rust 필터는 이 차이를 보존한다. E3의 일반 clause 축약은 이번 구현에 활성화하지 않았다.

## 4. 경로 길이에 대한 귀납: 모든 보드를 한 번에 다루기

완전한 유한 자세 집합을 `Q`, 자세 수를 `N>0`이라 한다. `S_q`는 그 보드에서 허용된 entry이며 자세가 자유로운 family다. 도달 family를

\[
R_0(q)=S_q,
\]

\[
R_{k+1}(q)=R_k(q)\cup
\bigcup_{p\to q}\left(R_k(p)\cap\mathcal E_{pq}\right)
\tag{R1}
\]

로 정의한다.

**정리 R2.** 모든 보드 B에 대해 `B∈R_k(q)`인 것과 B에서 q까지 길이 k 이하의 합법 경로가 있는 것은 동치다.

**기저 k=0.** `S_q`가 정확히 길이 0의 entry다.

**귀납 단계.** 첫 항은 이미 k 이하인 경로다. 나머지 항은 같은 B에서 p까지의 경로와 활성인 마지막 간선을 결합한다. 반대로 k+1 이하의 경로는 이미 짧거나, 마지막 간선 앞까지 k 이하이다. 고정 B에서의 membership 평가가 union/intersection을 OR/AND로 보존하므로 이 논증이 모든 B에 동시에 성립한다. □

한 B의 경로에 cycle이 있으면 같은 자세에서 cycle을 제거할 수 있다. board는 한 미노의 이동 도중 변하지 않고 이 모형은 입력 이력을 읽지 않는다. 그러므로 단순 경로 길이 `N−1`이면 충분하고 `R_{N−1}=R_*`가 정확한 최소 고정점이다. `N=0`은 모든 family가 빈 별도 경우다.

새 도달분 `ΔR`만 worklist로 전파해도 누락 없이 고정점까지 완료하면 같은 결과다. 현재 Rust는 이 delta 전파를 사용한다. 자원 한도 때문에 중단되면 `Refused/WorkLimit`를 반환하며 미완료 family를 false로 게시하지 않는다.

여기서 단조성의 대상은 `R`이다. **보드 점유에 대한 도달성은 단조가 아니다.** 기존 T/SRS90 반례에서 `0x4002`의 목표 `0x201c`는 도달 불가지만 bit 11을 더한 `0x4802`에서는 앞 킥이 막혀 도달 가능하다. 이 사례를 새 family 검산에도 넣었다.

### 4.1 버리면 거짓 경로를 만드는 정보

`s→a`의 조건이 x이고 `a→t`의 조건이 `¬x`라고 하자. 두 간선은 각각 어떤 보드에서 가능하지만 한 보드에서 함께 가능하지 않다. 간선의 존재 여부만 합친 그래프는 거짓 경로를 만든다. family 경로곱은 `x∧¬x=false`로 정확히 거절한다.

따라서 보드가 달라질 때 가능한 자세/셀/간선의 합집합만 남기는 방식으로는 일반적인 의존성 증명이 닫히지 않는다. Signed Boolean 조건, 동치인 집합 family, 또는 그 정보를 잃지 않는 다른 표현이 필요하다.

## 5. 보드 비트 수에 대한 조합론적 귀납

임의의 Boolean family f는 한 비트 x로

\[
f=(\neg x\land f|_{x=0})\lor(x\land f|_{x=1})
\tag{B1}
\]

처럼 유일하게 나뉜다.

**기저 m=0.** family는 공집합 또는 빈 assignment 하나의 집합이다.

**m→m+1.** x=0과 x=1은 겹치지 않으며 전체를 덮는다. 두 cofactor는 m변수 family이므로 귀납 가정으로 정확하게 표현된다. B1은 전체 family를 정확히 복원한다. □

고정 변수 순서의 reduced ordered Boolean decision diagram은 이 분할을 공유한다. 두 자식이 같으면 x를 제거해도 식이 같고, 동일 `(x,low,high)` 노드를 공유하면 동일 함수를 공유한다. AND/OR/XOR/NOT 및 cofactor는 가장 앞 변수를 분할하여 더 짧은 suffix 문제로 내려가는 귀납으로 집합 의미를 보존한다.

이 원리는 기존 BDD 연구의 방법이다. 이번 기여는 새 일반 Boolean 이론을 주장하는 것이 아니라 Clearra의 ordered kick·ILC·row frame에 정확하게 연결하는 데 있다. [Bryant 1986](https://www.cs.tufts.edu/~nr/cs257/archive/randall-bryant/bdds.pdf)은 reduced decision graph와 연산의 기반 및 최악 크기의 한계를 설명한다. Symbolic relation과 고정점 계산의 일반 배경은 [Burch 외의 원 논문](https://www.cs.cmu.edu/~emc/papers/Conference%20Papers/symbolic%20model%20checking%2010%20%20states%20and%20beyond.pdf)이다.

### 5.1 왜 5L 이상에도 적용되는가

증명 명제는 “작은 PC가 성공한다”가 아니다. **임의의 유한 비트 수, 모든 board assignment, 모든 자세에 대해 symbolic 식이 primitive의 의미와 같다**는 것이다. 높이 H+1은 W개의 새 비트를 추가한 모형이며 B1을 W번 적용할 수 있다. 그 높이의 shape/edge/entry를 다시 정확히 정의하면 R2는 새 자세 수에 그대로 적용된다.

새 행이 추가돼도 기존 도달성 값이 유지된다는 주장은 하지 않는다. 새 장애물과 경로를 새 조건식 안에서 정확히 다룬다는 증명이다. 줄 삭제의 rank/select 역시 아래의 별도 행 귀납으로 닫힌다. 실제 자료형의 좌표 범위·보드 표현 범위는 이 수학 명제와 별개의 구현 제한이다.

## 6. 실제 도달 계산 전에 미노와 무관한 영역을 제거

E2로 불가능한 간선을 제거한 potential graph에서 목표 q의 모든 predecessor를 `K_q`라 하자. 다음 읽기만 모아 셀 집합 `D_q`를 만든다.

1. `K_q`의 source/target 충돌 셀과 seed 판정 셀.
2. 살아 있는 ordered edge를 선택하는 데 필요한 **모든 앞 후보**의 충돌 셀.
3. 목표 grounding이 읽는 아래 자세의 셀.

**정리 D1.** 같은 인증 cube 안에서 두 보드가 `D_q`에서 같으면 목표 lock의 Boolean 답이 같다.

**증명.** 실제 목표 경로의 모든 자세는 potential predecessor이므로 `K_q` 안에 있다. 밖으로 나갔다가 목표로 돌아오는 자세도 정의상 predecessor다. `D_q`에서 같은 두 보드는 이 cone의 seed와 모든 실제 간선 guard, 목표 grounding을 같게 평가한다. R2의 경로 귀납을 양쪽에 적용하면 목표 도달성이 같다. □

이 절차는 board별 실제 도달성을 먼저 계산하지 않는다. 프로필/미노/geometry/cube 단위의 가능 간선과 읽기만 사용한다. 이미 있는 [static_lock_dependency_mask](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_independence.rs)의 validation·역방향 adjacency·cone·읽기 연산을 재사용하여 E2 필터를 앞에 붙였다.

6L O 지붕 검산에서는 기존 전역 mask가 읽던 외부 변동 셀 8개가 cube 인증 후 0개가 됐다. 가능한 외부 보드 256개에서 답이 동일함을 독립 oracle로 확인했다. **0개 mask 자체는 답이 일정하다는 증거이며, true/false의 값까지 주는 증거는 아니다.** 이 fixture의 실제 값은 별도 oracle에서 false였다.

`D_q`는 충분한 영향 상한이며 반드시 최소는 아니다. 여러 목표는 같은 potential graph의 SCC/condensation과 ancestor/read 요약을 공유할 수 있다. 이 재사용과 E3 흡수는 다음 구조 개선 대상으로 남긴다.

## 7. 최소 함수 의존성과 공동 변수 제거

full cube에서 셀 c에 대한 Boolean 미분을

\[
\partial_cf=f|_{c=0}\oplus f|_{c=1}
\]

로 정의한다. `∂_cf≠false` iff c 하나를 바꾸어 답을 바꾸는 assignment가 존재한다. Reduced ordered diagram에 실제로 남은 변수 집합은 정확한 global functional support다. 변수가 없으면 값이 영향을 받을 수 없고, 변수가 남은 노드는 서로 다른 두 자식을 가지므로 앞 context와 뒤 witness를 골라 차이를 보일 수 있다.

그러나 상관된 `Γ`에서는 독립적인 한 비트 toggle만 검사해서 여러 변수를 동시에 없애면 안 된다. 예를 들어

\[
\Gamma=\{(0,0),(1,1)\},\qquad f=x_0
\]

에서 합법적인 단일 toggle 비교는 공허하다. 두 비트를 모두 지우면 false와 true 상태가 합쳐진다.

남길 변수 집합을 N, 지울 집합을 J라 하면 정확한 조건은

\[
\forall x,y\in\Gamma:\;x|_N=y|_N\Rightarrow f(x)=f(y).
\]

이를 계산 가능한 식으로 쓰면

\[
P=\exists_J(\Gamma\land f),\qquad
Q=\exists_J(\Gamma\land\neg f),
\]

\[
\boxed{\text{J를 공동 제거 가능}\iff P\land Q=\mathrm{false}.}
\tag{D2}
\]

**증명.** 교집합에 남은 assignment가 있으면 같은 N 값에 양성/음성 completion이 함께 있어 함수가 잘 정의되지 않는다. 교집합이 비면 모든 fibre가 양성만, 음성만, 또는 빈 집합이므로 하나의 값을 부여할 수 있다. □

새 커널은 existential projection을 cofactor와 OR로 계산한다. 이 반례에서 각 비트 하나의 제거는 가능하지만 두 비트 동시 제거는 불가함을 검산했다. `support(Γ∧f)`는 Γ 자체를 표현하는 변수까지 포함할 수 있어 Γ 내부 최소 영향 집합으로 부르지 않는다.

## 8. ILC와 줄 삭제를 보드 family에 치환

서로 다른 operation ID를 `i=0,…,n−1`, 원래 행 footprint를 `P_i`, 실행 여부 변수를 `x_i`라 한다. 같은 종류의 미노도 ID를 합치지 않는다. base와 footprints는 쌍별로 겹치지 않으며 초기 full row는 정규화돼 있다.

최종 union에서 실제로 full인 행 집합 F와 contributor `C_r`에 대해

\[
\delta_r(x)=[r\in F]\land\bigwedge_{i\in C_r}x_i,
\qquad \sigma_r=\neg\delta_r
\]

로 행 삭제/생존을 표현한다. 부분 Build의 full이 아닌 행은 삭제하지 않는다. 원래 행 r의 생존 rank는

\[
\rho_r=r-\sum_{d<r}\delta_d.
\]

원래 셀 점유 `U_{a,r}(x)`는 base의 참 상수 또는 유일한 owner 변수다. 물리 보드 셀은

\[
\boxed{
\chi_{a,b}(x)=\bigvee_r
\left(\sigma_r(x)\land U_{a,r}(x)\land[\rho_r(x)=b]\right).
}
\tag{C1}
\]

### 8.1 Prefix를 만들지 않는 행 귀납

`L_r(k)`를 첫 r개 원래 행 중 생존한 행이 정확히 k개라는 조건이라 하자. 기저는 `L_0(0)=true`, 나머지는 false다. 행 r을 처리하면

\[
L_{r+1}(k)=
(L_r(k)\land\delta_r)
\lor
(L_r(k-1)\land\sigma_r),
\tag{C2}
\]

범위 밖 k의 항은 false다. r행을 삭제하면 생존 수가 유지되고 남기면 하나 늘어나므로 두 경우는 완전하고 배타적이다. 행 수에 대한 귀납으로 C2가 정확하다. `select[r][k]=L_r(k)∧σ_r`를 공유하면 C1과 모든 목표의 이동 좌표를 구성할 수 있다.

이 recurrence는 행과 rank를 순회한다. **전체 `2^n` subset을 생성하는 단계가 없다.** Boolean 식 자체의 크기까지 다항식이라고 보장하는 것은 아니다.

### 8.2 타일링 이전 family를 이후에도 재사용하는 정리

operation assignment에서 물리 보드로 가는 사상을 `β(x)=χ(x)`라 하자. 집합의 역상 `β⁻¹`은 union/intersection/complement를 모두 보존한다. 따라서 R1의 각 단계에 역상을 취하는 것과 먼저 edge/seed에 χ를 치환한 뒤 같은 recurrence를 계산하는 것이 같다. 경로 길이 귀납으로

\[
\boxed{\text{보드 family 계산 후 치환}
=\text{치환된 보드 조건으로 family 계산}.}
\tag{C3}
\]

목표 자체도 줄 삭제로 움직인다. `τ_{iv}(x)`를 **i의 정책에서 허용한 realization**이라는 조건 `Allowed_i(v,x)`와, v의 종류·**전체 footprint**가 `Φ_{D(x)}(P_i)`에 일치하는 조건의 AND로 두면

\[
e_i(x)=\neg x_i\land\operatorname{ILC}_i(x)
\land\bigvee_v\left[
\tau_{iv}(x)\land\operatorname{Ground}_v(\chi(x))
\land R_v(\chi(x))
\right].
\tag{C4}
\]

모든 허용 realization을 OR한다. I/S/Z/O처럼 다른 orientation이 같은 셀 집합을 가질 수 있으므로 종류와 셀 equality만으로 orientation 제한을 대신하지 않는다. 이전 문서의 원래 행 역상 유일성과 fixed-cover 계약으로 ILC witness와 목표 pose가 같은 operation을 가리킨다. 검산기의 `operation_family`에서는 허용된 동일 kind의 모든 full pose와 injective row-map의 네 셀 equality를 확인한다. 이 유한 fixture의 ILC realization 검사를 이 equality에 결합한 것이며, 다른 target/orientation 정책까지 자동 승인하는 외부 API가 아니다.

### 8.3 치환의 두 가지 필수 조건

첫째, cube에 한정해 만든 family를 치환한다면 모든 합법 `x∈Γ`에서

\[
\beta(x)\in\mathcal B(O,V)
\tag{C5}
\]

임을 증명해야 한다. cube 밖을 false로 둔 함수를 무조건 재사용하면 유효한 prefix를 잃을 수 있다. full cube에 대해 만들었다면 이 의무는 자동이다.

둘째, Boolean 치환은 **동시 치환**이다. 원래 변수 j를 `χ_j`로 바꾸되 삽입한 `χ_j` 안의 변수에 같은 치환을 다시 적용하지 않는다. BDD의 변수 순서를 보존하려면 단순 슬롯 교체가 아니라 `ITE(χ_j,compose(high),compose(low))`로 재구성해야 한다. 새 커널은 이 규칙을 따른다.

물리 영향 셀 `D_q` 밖의 미노라도 δ/rank/target을 바꾸면 간접 영향이 있다. C4의 목표 v 자체가 변하므로 **한 고정 q의 mask만 사용하는 것으로는 부족하다.** 같은 인증 cube와 frame에 결박된 `D_v`에 대해

\[
D_i=\bigcup_{v:\;\Gamma\land\tau_{iv}\text{가 satisfiable}}D_v
\]

를 사용한다. 가능 목표를 아직 모르면 모든 허용 v를 포함하는 과대근사로 합친다. 안전한 operation 영향 상한은 `χ_c`의 support(`c∈D_i`), pending·ILC·target·grounding 조건의 support를 합친 것이다. 동등하게 C4의 각 목표 항별 support를 구한 뒤 합칠 수도 있다. `τ_iv`의 support만 추가하는 것으로 다른 목표의 실제 충돌 읽기를 대체할 수는 없다.

어떤 operation j의 변경이 이 모든 물리 셀 식과 목표/ILC 조건을 바꾸지 않으면 D1과 함수 합성에 의해 i의 답도 바뀌지 않는다. 이 방법은 직접 충돌 의존성과 줄 삭제 이벤트의 간접 의존성을 분리하여 공유한다.

## 9. 내장 프로필의 유한 sky 경계 증명

R2는 주어진 유한 graph에 대해 완결돼 있다. 실제 이동 계약에서 유한 template을 얻는 단계도 정확해야 전체 물리 결론을 말할 수 있다.

이 절의 최초 검산 시점에는 [ReachabilityTemplate](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability.rs)이 `ceiling=H+최대 정규화 하향 kick`을 쓰고, 유효한 `anchor_y≥H` 자세들을 모두 seed로 두었다. 이는 **all-sky entry** 계약이다. 특정 게임의 한 spawn 위치나 manipulation timer와 같다고 가정하지 않는다. 현재 범용 compiler는 실제 목록이 아래 충분조건을 만족할 때에만 이 작은 경계를 사용하고, 그 외에는 [범용 상공 정리](parametric-kick-equivalence-2026-11-22.md)에 따라 `H+D+U`를 사용한다.

단순히 “마지막으로 sky에서 내려온 source는 H+D 이하다”만 말해서는 부족하다. 잘린 **앞선 킥 후보**가 성공했을 수 있다면 later candidate를 잘못 활성화할 수 있기 때문이다. 최초 검산의 raw oracle도 같은 ceiling 규칙을 사용했으므로 당시 두 구현의 답이 같다는 사실만으로 이 의무를 대체하지 않는다. 다음 증명은 그 빈틈을 닫는다. 현재의 독립 all-sky raw oracle은 production의 축약 capability를 호출하지 않고 원본 오프셋에서 일반 경계 `H+D+U`를 직접 계산한다. 별도의 explicit-entry/window oracle은 자신의 기존 유한 진입 계약을 유지한다.

### 9.1 높이와 보드를 열거하지 않는 유한 조건

정규화된 활성 kick의 수직 변위에 대해

\[
D=\max(0,\max(-dy)),\quad U=\max(0,\max dy),\quad C=H+D
\]

라 한다. shape의 local min x/min y가 0이며 다음 조건을 만족한다고 하자.

**A.** `U≤D`.

**B.** 각 ordered 후보 j와 `0≤t≤D`에서 `t+dy_j<0`이고 어떤 앞 후보 k가 `t+dy_k>D`라면, 다음 조건의 앞 후보 h가 존재한다.

\[
h<j,\qquad dx_h=dx_j,\qquad 0\le t+dy_h\le D.
\tag{S1}
\]

B에서 source는 `H+t`, j는 높이 H 아래로 들어오는 후보, k는 천장 밖으로 잘린 후보다. h는 j와 방향·x가 같으므로 j가 가로로 유효하면 h도 유효하다. h는 `H≤y≤C`의 빈 sky이며 template에도 남아 있으므로 j보다 먼저 성공한다.

**보조정리 S2: low source의 이동 보존.** `y<H`인 source의 모든 회전 후보는 A에 의해 `y+dy≤H−1+U<C`다. 위쪽으로 잘리는 후보가 없고, floor/벽/점유 조건은 양쪽에서 같으므로 first success가 같다. down/left/right도 같다. □

**보조정리 S3: sky→low 교차 보존.** down의 source는 H다. 회전의 source를 `H+t`라 하면 `t+dy_j<0`이므로 `0≤t<D`; source와 target은 유한 범위 안이다. 무한 모델의 low first success는 유한 모델에도 남는다. 역으로 유한 모델만 j를 선택했다고 가정하면 삭제된 earlier 성공 k가 필요하다. S1은 남아 있는 earlier sky 성공 h를 주므로 유한 모델이 j를 선택했다는 가정과 모순이다. □

**정리 S4: 모든 높이의 all-sky Boolean 동치.** 한 모델에서 low 목표에 이르는 witness 경로를 골라 **마지막 sky 방문** 이후만 남긴다. 그 sky source는 S3에 의해 유한 범위 안에 있고 양쪽의 seed다. sky→low 간선은 S3, 이후 모든 low source 간선은 S2로 보존된다. 반대 모델에서도 같은 논증을 적용하므로 양방향 도달성이 같다. 목표 grounding은 바닥/아래쪽 충돌만 읽으므로 Boolean lock 가능성도 같다. □

이 증명은 sky→sky 간선의 완전한 일치를 요구하지 않는다. 모든 sky 자세가 이미 seed라는 계약 덕분이다. 고정 spawn에 자동 적용할 수 없는 이유이기도 하다.

### 9.2 실제 프로필의 충분조건 확인

| 프로필 | J/L/S/T/Z의 `(U,D)` | I의 `(U,D)` | O의 `(U,D)` |
| --- | --- | --- | --- |
| NoKick | `(1,1)` | `(2,2)` | `(0,0)` |
| Srs90 | `(3,3)` | `(3,3)` | `(0,0)` |
| SrsPlus | `(3,3)` | `(3,3)` | `(0,0)` |
| SrsX | `(3,3)` | `(3,3)` | `(0,0)` |
| Jstris180 | `(3,3)` | `(3,3)` | `(0,0)` |

NoKick도 normalized anchor는 방향에 따라 움직인다. Raw zero kick에서 U=D=0을 추론하면 안 된다. Jstris180의 O에는 회전 entry가 없다.

현재 표에서 B의 비자명한 경우는 SrsX J/L/S/T/Z의 Right↔Left half-turn이다. `t=2`에서 index 10의 `dy=−3` 앞에 있는 `dy=+2` 후보가 잘릴 수 있다. 하지만 index 0은 같은 dx, `dy=0`인 retained sky 후보여서 index 10을 먼저 막는다.

추가한 `board_family_builtin_ceiling_profiles_satisfy_height_independent_conditions`는 위 분류를 상수 문자열로 확인하지 않는다. **기존 프로필 소유자에서 실제 후보를 읽고 A/B 및 shape min x/min y를 계산한다.** 35개 프로필·미노 조합과 10개 비자명 교차 조건을 통과했다. 이 유한 검산은 높이 표본 실험이 아니라 모든 H에 적용되는 충분조건 확인이다.

### 9.3 Custom kick에 대한 구체적 반례

O의 모든 회전 입력에 첫 후보 `(0,+7)`, 둘째 후보 `(+4,0)`를 준다고 하자. 폭 6, H=5에서 x=2,3의 모든 행을 벽으로 채우고 `(4,2),(5,2)`에 지붕을 둔다. 목표 O는 `(4,0)`이며 왼쪽 통로의 `(0,0)`은 도달 가능하다.

무한 sky에서 첫 후보는 항상 자유로워 두 번째 수평 점프가 선택되지 않는다. 가운데 벽과 지붕 때문에 목표는 도달 불가다. 하지만 D=0을 이용해 천장을 H로 자르면 `(0,0)`에서 첫 후보 y=7이 사라지고 두 번째 후보가 밀폐 공간으로 진입한다. **같은 board에서 false가 true로 바뀐다.**

이 반례는 현재 builtin의 버그가 아니다. A/B 없이 모든 kick table에 천장 식을 일반화하는 주장을 반박한다. Rust에서도 천장 5의 잘못된 true와 low-source 후보를 모두 보존하는 천장 12의 false를 대조한다. 이 custom 규칙의 sky→low는 down뿐이어서 천장 12는 마지막 sky suffix 논증으로 무한 모델과 같다.

Imported/custom kick, 새로운 entry 계약, 좌표 표현의 한도를 벗어난 높이는 이 검산 결과를 그대로 승계하지 않는다. 프로필 내용·enabled action·shape embedding·entry·frame·target을 인증의 결박에 넣어야 한다.

## 10. Rust 구현과 재사용 경계

| 파일 | 역할과 기존 연산 재사용 |
| --- | --- |
| [reachability_boolean_family.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_boolean_family.rs) | 유한 Boolean family의 reduced ordered DAG. AND/OR/XOR/NOT, cofactor, 동시 치환, 존재 양화, 정확한 global support |
| [reachability_board_family.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_board_family.rs) | 기존 `ReachabilityTemplate`의 geometry/ordered edge/seed/ground 의미를 소비. family 전파, symbolic row rank/select, operation 조건과 독립 oracle 대조 |
| [reachability_independence.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_independence.rs) | 기존 static mask 내부 연산에 cube E2 필터를 적용하는 test-only 경로. 기존 전역 호출은 필터 없는 같은 동작 |
| [reachability.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability.rs) | 두 검산 모듈의 `cfg(test)` 연결 |

현재 [reachability_symbolic_target.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_target.rs)는 이미 생성한 최대 64개 concrete board를 lane으로 묶어 정확하게 계산한다. 이번 family는 assignment 목록을 생성하지 않고 Boolean 식 자체를 전파한다. 기존 standard-bag decision 구조는 공급/hold/policy를 소비하므로 보드 family의 변수 의미와 동일시하지 않는다.

제품 통합 시 새 이동 엔진을 병렬로 유지하는 것이 목표가 아니다. 검산으로 확인한 Boolean 대수 backend와 기존 geometry/guard 소유자를 연결하고, 같은 프로필/타깃/row 조건의 연산 결과를 공유해야 한다. BDD마다 ID가 소속된 manager가 있으며 서로 다른 manager의 ID를 섞을 수 없다.

현재 검산기는 64개 이하 변수, 폭 1~10, 높이 1~6, 4,096개 이하 pose 등 명시된 제한이 있다. 이는 검산기의 제한이며 수학 정리의 높이 제한이나 새 공급 인코딩 제안이 아니다. Node/work budget 및 관측한 취소는 `Refused`로 분리했다. 일부 작은 보조 배열은 `vec!`/`clone`/`collect` 등으로 할당하며 모든 allocator 실패를 typed refusal로 바꾸지는 않는다. Row/operation helper의 취소는 Boolean manager의 선택적 취소와 한도에 의존한다. 제품의 완전한 메모리 회계·fallback·cancellation plumbing을 완료했다고 주장하지 않는다.

## 11. 재현 가능한 검산 결과

Rust 1.98.1, `placement-dependency-pc`, debug test profile에서 실행했다. 큰 matrix는 `available_parallelism()`이 보고한 **9개 worker**를 사용했다. 시간은 테스트 wall time이며 제품 성능 비교값이 아니다.

| 검증 | 실제 범위와 결과 |
| --- | --- |
| Boolean kernel | 6 tests 통과. 3변수 Boolean 함수 256개 전체, AND/OR/XOR 함수 쌍 196,608개, cofactor/존재 양화/동시 치환/support 검산, refusal/cancellation/64변수 경계 |
| Board family | 9 tests 통과, 1.50초. 1~6L × 7미노 × 5프로필 = 210 case, 보드 입력 45,360건, 네 셀이 보드 안에 모두 있는 목표 lock 비교 1,111,200건, 불일치 0 |
| 높이 독립적 천장 조건 | 35개 프로필·미노 조합과 10개 비자명 교차에서 A/B 확인. Custom upward kick 반례에서 부적절한 천장과 충분한 천장의 true/false 차이 재현 |
| Cube 간선 정리 | 3셀 cube 27개와 source/target/prior mask 조합 110,592개에서 E2와 직접 first-success 열거 일치 |
| 5L/6L scoped mask | 7미노 × 5프로필, 6개 변동 셀의 64보드, 목표 판정 34,560건. 양성 6,410 / 음성 28,150, 같은 mask signature 내 불일치 0 |
| 6L O 지붕 | 전역 mask의 외부 변동 셀 8개 → scoped mask 0개. 외부 보드 256개 대조 |
| 원래 전역 mask / earlier competitor | 관련 기존 회귀 2 tests 통과 |

Board family matrix는 폭 4에서 맨 아래/맨 위 행의 셀을 변수로 두고 나머지는 빈칸으로 고정했다. 1L은 4비트, 2~6L은 8비트의 cube 전수검사다. **5L/6L의 모든 가능한 보드를 검사했다는 뜻이 아니다.** NoKick, Srs90, SrsPlus, SrsX, Jstris180 및 I/O/T/S/Z/J/L를 포함한다.

이 matrix의 symbolic compiler는 54,034개 potential edge를 cube 식만으로 제외했고, case별 가장 큰 BDD arena는 terminal 포함 1,124 nodes였다. 간선 감소량은 이 입력 집합의 구조적 작업량이며 대규모 탐색의 속도 배율로 환산하지 않는다.

줄 삭제와 타일링 이전 재사용은 별도 fixture로 검산했다.

| Fixture | 확인한 결과 |
| --- | --- |
| 폭 2, 6L, 세 O operation | raw board 12변수의 관계를 타일링 전에 한 번 컴파일한 뒤 row 식을 동시 치환. 8개 assignment, prefix별 reachability 호출 0, BDD arena 101 nodes |
| 같은 fixture의 목표 frame | 모든 물리 보드는 비어 있어도 위 O는 아래 행을 먼저 삭제해야 목표가 바닥에 온다. `e_i=¬x_i ∧ ∧_{j<i}x_j`와 정확히 일치. 같은 종류 ID를 합치면 잃는 차이를 보존 |
| 폭 4, 6L, 여섯 O operation | partial clear가 있는 64개 subset, operation 판정 384건을 raw oracle와 대조. relation 컴파일 1회, prefix별 relation 컴파일 0, BDD arena 347 nodes |
| row 귀납 | 위 6L, 1L I, non-full Build row, inverse T/L gap fixture의 compaction/delete/select 대조 |
| 오류 방지 반례 | `x→¬x`의 거짓 경로, 점유 추가로 later kick 활성화, Γ에서 단일 제거와 공동 제거의 차이, 자원 거부가 false가 아님 |

여기서 arena node 수는 살아 있는 최소 결과 graph만의 크기가 아니라 계산 중 intern한 중간 노드를 포함한다. 현재 커널에 GC나 제품 cache 예산 정책을 추가한 결과도 아니다.

재현 명령은 저장소 루트에서 다음과 같다. 환경에 맞는 Rust toolchain과 허용된 Cargo 출력 경로를 사용한다.

```sh
cargo test -p clearra-core-executor --lib --features placement-dependency-pc boolean_family -- --nocapture
cargo test -p clearra-core-executor --lib --features placement-dependency-pc board_family -- --nocapture
cargo test -p clearra-core-executor --lib --features placement-dependency-pc combinatorial_cube -- --nocapture
cargo test -p clearra-core-executor --lib --features placement-dependency-pc static_target_masks_preserve_every_outside_bit_change -- --nocapture
cargo test -p clearra-core-executor --lib --features placement-dependency-pc earlier_kick_competitor_outside -- --nocapture
```

위 집중 검증은 합계 20 tests 통과, 실패 0이다. 원시 로그는 `_local/artifacts/combinatorial-proof-20261122/`에 남겼다. 기존 기능의 dead-code 경고 3종이 출력됐고, 이번 검산의 실패로 취급하거나 unrelated 수정 대상으로 삼지 않았다.

## 12. 증명으로 확보한 것과 다음 구조 개선

**확보한 것:** 임의의 유한 보드 family에서 정확한 ordered 이동 조건을 만들 수 있다. 실제 도달성 전에 안전하게 버릴 간선과 읽지 않아도 될 셀을 인증할 수 있다. 필요하면 Boolean family로 관계를 완성하고, 줄 삭제·target frame을 operation 식으로 치환해 전체 prefix 선생성을 생략할 수 있다. 같은 종류의 ID, signed 조건, correlation과 guard priority는 보존한다.

**아직 보장하지 않는 것:** 모든 관계를 작은 단순 선행 DAG로 표현하거나, 모든 타일링에서 작은 BDD를 만들거나, 도달 관계의 수학적 closure 비용을 항상 없앨 수 있다는 정리는 없다. 식의 간결성과 표현의 정확성은 별개다. 현재 코드는 “매 prefix concrete BFS”를 요구하지 않는 구성적 증거이며, 한 번의 symbolic closure도 전혀 필요 없다는 주장은 아니다. 검산 compiler는 E2로 간선을 거른 뒤 전체 template의 family를 계산한다. 별도로 검산한 target cone·받침 선거절을 모두 결합한 최저비용 제품 경로가 이미 완성된 것은 아니다.

Board family의 assignment 수를 큐 성공 확률로 바로 해석해서도 안 된다. 원래 큐의 공급 가중치와 hold·관측·concrete operation 선택을 기존 [정책/투영 증명](fixed-tiling-ilc-dependency-proofs-2026-11-22.md)에 따라 결합해야 한다. 특정 큐가 필요 없는 소비자는 projected counting을 사용하고, see-7은 reveal 전에 같은 관측에서 같은 concrete action을 선택하는 제약을 유지한다.

다음 작업은 큰 비용을 일으키는 중복 연산을 줄이는 순서로 나눈다. 단순 자료구조 크기·파일 수·행 수·정적 문구 검사는 이 계획의 gate가 아니다.

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, 구현 내용: 기존 grounding/row-frame 계산으로 `Γ∧Ground_v=false`인 목표를 이동 closure 전에 제외하고, pending operation의 모든 허용 목표가 제거됐을 때만 operation 음성을 승인한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_independence.rs`, 구현 내용: E3 blocker 흡수를 E2와 함께 인증하고, 프로필/geometry/cube 단위 potential graph 결과를 목표 사이에서 공유한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_static_influence.rs`, 구현 내용: SCC/condensation과 predecessor/read 요약의 계산 소유자를 기존 template 수명에 결박하여 같은 간선 closure를 타깃마다 재생성하지 않는다. 실제 소유 파일은 기존 모듈 경계를 확인한 뒤 정한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/placement_poset.rs`, 구현 내용: 기존 `CandidateGeometry`의 row contributor/ILC realization 계산에서 C1/C2의 공유 row-event 식을 공급하고 같은 frame을 별도 public 경로에서 재계산하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_target.rs`, 구현 내용: 기존 exact guard/target 의미에 검산된 Boolean backend를 결합하고, full-cube/부분 cube/domain 인증의 수명을 구분한다. 새 독립 이동 엔진을 제품에 추가하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_prefix_orders.rs`, 구현 내용: `2^n` prefix 선생성 대신 정확한 operation guard와 공동 projection 조건을 소비하고, 불완전/거부 결과를 exact negative로 승격하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy.rs`, 구현 내용: 기존 oracle/see-7의 concrete operation·공급·관측 phase를 새 guard의 동일 의미에 연결한다. 같은 kind 합치기는 D2/정책 동형 인증 후에만 허용한다.
- [ ] 수정 파일: `tools/clearra-manage/`, 구현 내용: 제품 통합 후 동일 요청/프로필/스레드/출력 조건의 큰 P7P4 및 5L 이상 작업에서 관계 생성 수·도달성 호출·재사용·전체 시간·메모리·거부율을 측정하는 유한 실험을 기존 관리 경로에 등록한다.

E2/E3의 프로필 구조 작업과 row-event/ILC 공유 작업은 병렬로 진행할 수 있다. 정확한 symbolic backend 통합은 두 결과의 결박을 소비한다. Prefix/queue 소비자는 완성된 operation 의미를 공유한 뒤 연결하며, 제품 성능 실험은 그 다음이다. 이 순서가 증명을 위해 모든 배치 순서를 다시 탐색하는 비용을 제품에 고착시키지 않도록 한다.

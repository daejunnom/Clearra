# 고정 타일링의 ILC 의존성: 높이 귀납, 조기 거절, 정책과 prefix 축약

작성일: 2026-11-22 KST

소스 대조 기준: `f1c57e4eab23734fcc6723c865dc50a1490b4d54` (`codex/v091-placement-dependency`)

**상태: 명시된 모형 아래의 수학 증명과 소스 대조를 완료한 문서다.** 제품 구현, 새 테스트 프로그램, P7P4 실행, 성능 측정, Lean 등의 기계 증명은 이번 작업에 포함하지 않았다. 정리의 높이 독립성과 현재 제품의 지원 높이는 구분한다. 이전의 4L P7P4 성공은 실행 증거이며 수학적 귀납의 기저나 모든 5L 이상 제품 실행의 인증이 아니다.

**후속 작업:** [보드 집합의 조합론적 구성과 Rust 검산](combinatorial-board-family-proof-2026-11-22.md)은 이 문서의 완전한 operation 명제 `e_i(S)`를 보드 비트·회전 원점·ordered kick에서 실제로 구성한다. 경로/비트 귀납, 도달성 이전의 cube 간선 제거, 공동 변수 제거, row-map 치환, 내장 프로필의 모든 높이 all-sky 천장 증명과 1~6L 실행 검산을 추가했다. 해당 후속 작업의 테스트 코드와 이 문서 작성 당시의 문서 전용 범위를 구분한다.

## 0. 이번 범위와 기존 계획의 정정

대상은 **ILC를 사용하는 고정 타일링의 Boolean 구축/PC 존재성과 oracle·see-7 확률**이다. 해당 계약을 만족하는 Build/Setup 소비자는 내부 연산을 공유할 수 있다. damage·spin 탐색에 의존성 분석을 새로 적용하는 작업, finesse 입력 비용/최단 경로, B2B·점수·CountAll 확장은 이번 범위에서 제외한다. 별도 제품에서 이미 쓰는 공통 형상·규칙·공급 연산을 읽고 재사용하는 것은 이 제한과 양립한다.

기존 [관계 증명 의무](tetris-relation-proof-obligations-2026-11-20.md), [Rust 통합 로드맵](rust-core-and-relation-roadmap-2026-11-20.md), [실행 장부](../v0.9.0-placement-dependency-rust-release-plan.md)의 역사적 범위는 보존한다. 현재 수학 작업의 우선순위와 아래에서 완료한 의무는 이 문서를 따른다. 과거 체크리스트의 미체크 항목 수를 현재 남은 작업 수로 해석하지 않는다.

핵심 결정은 다음과 같다.

1. Build에 이미 있는 ILC 선행 폐쇄를 일반화한다. 같은 순환 검사를 새 엔진으로 중복 제작하지 않는다.
2. 고정 footprint와 원래 행 contributor를 타일링 이전/도중부터 재사용한다. 관계를 만들기 위해 모든 배치 순서를 다시 탐색하는 것을 목표로 삼지 않는다.
3. see-7은 concrete operation 선택을 보존하는 의존성 관계로 정의한다. 별도 finesse action/입력 경로 객체는 이 Boolean 의미의 필요조건이 아니다.
4. 모든 prefix 선생성을 없애는 정확한 표현과, 실제 상태 수를 줄이는 동치 조건을 구분해 증명한다.
5. CEGIS의 안전한 거절 부류와 전체 실패를 포괄하는 완전한 인증 형식을 구분한다. 작은 규칙 몇 개가 모든 실제 P7P4 실패를 이미 포괄한다고 주장하지 않는다.

### 0.1 소스에서 이미 있는 연산과 실제 활성 범위

| 연산 | 현재 소유자와 범위 | 이번 결론 |
| --- | --- | --- |
| ILC 단조 폐쇄 | [realization_feasibility.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/realization_feasibility.rs)의 `dependency_relaxation_stalls` | contributor→삭제 행→가능 operation을 반복하는 기존 연산을 공유 |
| Build 확률의 ILC 분석 | [build_probability.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/build_probability.rs)의 `analyze_inverse_dependencies`; [extended_build_probability.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/extended_build_probability.rs) | 해당 연결은 `cfg(test)`이며 일반 제품에서 항상 활성인 것은 아님 |
| 일반 BuildUp prepass | [buildup.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs) | `local-search-ab`에 한정된 연결과 제품 기본을 구분 |
| 완전 subset grounding prepass | `RealizationFeasibility`의 별도 prepass | 기본 상수 false. 작은 단조 폐쇄와 별개 |
| 더 강한 선행 poset | [placement_poset.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/placement_poset.rs), `test`/`local-search-ab` prototype | frame·support·movement 증거의 기존 연산을 조사한 뒤 재사용 |
| 정확한 operation 관계 | [symbolic_prefix_orders.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_prefix_orders.rs), bounded Board64 PC | upper/lower와 정제 결과를 보유. `placement-dependency-pc`의 `live_operations_after`가 operation index와 kind를 함께 제공 |
| 공급·관측 | [queue_observation_policy.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy.rs) | 같은 관측에서 child를 선택한 뒤 reveal하는 기존 평가 순서를 활용 |

빈 필드 P7P4의 현재 geometry는 `advanced_domain = advanced_analysis_enabled && catalog.initial_board() != 0` 때문에 `compile_minimum` 경로다. 따라서 빈 P7P4를 실제로 측정한 표현처럼 “APDP filter 통과 후 탈락”이라고 쓰면 부정확하다. 아래에서는 **기하 단계 이후 탈락**을 다루며, APDP를 활성화한 별도 요청/실험과 구별한다. 근거는 [geometry.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs)의 `FamilyCompiler::advance`와 [geometry_domain.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_domain.rs)다.

## 1. 공통 모형과 양화

폭을 W, 원래 행 높이를 H라 하고 바닥 행을 0으로 둔다. 초기 full row를 먼저 정규화한 점유를 B₀, concrete operation ID 집합을 I={1,…,n}, 종류를 κ(i), 원래 좌표의 네 셀 footprint를 Pᵢ라 한다. B₀와 모든 Pᵢ는 쌍별로 겹치지 않는다. 같은 종류의 두 미노도 서로 다른 ID와 footprint를 가진다.

F는 B₀∪⋃ᵢPᵢ에서 실제로 가득 차는 원래 행의 집합이다. Build의 부분 목표에서는 full이 아닌 행을 F에 넣지 않는다. r∈F의 contributor는 Cᵣ={i : Pᵢ가 r에 셀을 가진다}다. 초기 full row가 정규화됐으므로 새 삭제로 취급할 빈 contributor 행은 없다.

실행 집합 S⊆I에 대해

$$
U(S)=B_0\cup\bigcup_{i\in S}P_i,
\qquad D(S)=\{r\in F:C_r\subseteq S\}.
$$

삭제 집합 D의 생존 행 사상은

$$
f_D(r)=r-|\{d\in D:d<r\}|\quad(r\notin D)
$$

이고 Φ_D는 삭제 셀을 없앤 뒤 각 생존 셀의 y에 f_D를 적용한다. g_D는 생존 원래 행 목록에서 해당 순위를 고르는 역함수다.

eᵢ(S)는 해당 frame에서 i의 ILC 실현, 충돌 없음, grounding, 정확한 entry-to-lock 이동이 모두 성립하는 **완전한 Boolean 명제**다. 적용 가능한 모든 구체 실현의 존재 OR를 포함한다. 분석 미완료나 자원 거부를 false로 바꾼 것이 아니다. 관계 간선은 i∉S이고 eᵢ(S)일 때 S→S∪{i}다. 전체 I의 실행이 완료 목표이며, 다른 종료 조건은 별도로 명시한다.

lock 사이의 물리 Boolean 판정에 필요한 상태는 S와 고정된 규칙/entry 계약으로 충분하다고 가정한다. 이전 입력, last rotation, spin, 시간, finesse 비용을 소비하는 목표는 이 모형에 없다. supply·hold·관측 상태는 필요한 만큼 별도로 결합한다. 물리 실행이 어떤 타일링에 속하는지 주장할 때는 그 타일링의 Pᵢ들을 실행하는 경우로 한정한다.

## 2. 1L에서 모든 높이로 닫히는 귀납

귀납 명제는 “1L PC가 성공한다”가 아니다. 폭 10의 빈 1L는 10칸이므로 4칸 미노만으로 완전히 채울 수 없고, 여러 shape의 높이 조건도 이 기저에서 공허하게 거절된다. 대신 **모든 행 payload, 모든 삭제 마스크, 모든 길이의 부분 shape profile**에 대해 다음 정리를 증명한다. payload는 셀뿐 아니라 owner label을 포함할 수 있다. q=0의 빈 row tuple은 귀납을 위한 수학적 보조값이다.

### H1. Rank/select는 모든 높이에서 전단사다

f_D는 생존 원래 행과 {0,…,H−|D|−1} 사이의 증가 전단사이며 g_D가 역함수다.

**1L 기저.** 행 0이 생존하면 0→0이고, 삭제되면 양쪽이 빈 집합이다. 둘 다 전단사다.

**H→H+1.** 새 꼭대기 행 H가 삭제되면 하위 H행의 대응만 남는다. 생존하면 기존 생존 행 목록 뒤에 H−|D∩{0,…,H−1}|이라는 새 마지막 index를 붙인다. 기존 대응은 변하지 않고 새 값은 이전 최대보다 정확히 1 크다. 두 경우가 모든 마스크를 분할하므로 단사·전사·순서 보존이 성립한다. □

### H2. 동시 삭제와 연속 삭제의 합성은 같다

D∩E=∅인 원래 행 집합에 대해

$$
\Phi_{f_D(E)}\bigl(\Phi_D(B)\bigr)=\Phi_{D\cup E}(B).
$$

**1L 기저.** 유일한 행이 첫 삭제에서 사라짐, 둘째 삭제에서 사라짐, 살아남음의 세 경우에 양쪽 row sequence가 같다.

**H→H+1.** 하위 H행은 귀납 가정으로 같다. 꼭대기 행이 D에 있으면 첫 단계에서 없어지고, E에 있으면 첫 단계의 마지막 생존 행을 둘째 단계에서 없앤다. D∪E 밖이면 같은 payload가 하위 최종 sequence 뒤에 붙는다. 따라서 전체도 같다. □

산술로도 f_D가 증가하므로 e<r와 f_D(e)<f_D(r)가 동치다. 두 단계에서 r 아래로 세는 삭제 수는 각각 D와 E의 수이며 합이 D∪E의 수다. 연속하지 않은 여러 원래 행의 삭제도 포함한다. Φ_D는 생존 셀의 disjointness와 합집합을 보존한다.

### H3. ILC의 증가 행 tuple 열거에는 높이에 따른 누락이 없다

E(H,q)를 {0,…,H−1}의 길이 q 증가 tuple 집합으로 둔다. E(H,0)는 빈 tuple 하나이고 q>H면 공집합이다. q≥1일 때

$$
E(H+1,q)=E(H,q)\;\sqcup\;
\{(t_0,\ldots,t_{q-2},H):t\in E(H,q-1)\}.
$$

**1L 기저.** q=0에는 빈 tuple, q=1에는 (0), q>1에는 아무것도 없다.

**귀납 단계.** 임의의 tuple은 H를 포함하지 않거나 포함한다. 후자에서는 증가성 때문에 H가 마지막이고 앞의 q−1개가 하위 높이 tuple이다. 역으로 두 방식은 모두 유효하며 서로 겹치지 않는다. □

실제 shape의 q개 연속 physical row가 y,…,y+q−1이면 t_b=g_D(y+b)로 원래 tuple을 얻는다. H1로 증가하고 범위 안이며, 다음 행을 선택할 때 남은 q−b−1개를 위한 최대/최소 경계도 만족한다. 각 row의 x-pattern을 그대로 얹으면 실제 footprint를 복원한다. 실제 lock이 목표 안이고 B₀와 겹치지 않으면 row compatibility·column capacity 필요조건을 통과한다. 허용 삭제 행 필터는 실제 삭제를 모두 포함하는 F를 사용해야 한다.

새 꼭대기에 걸치는 shape는 q−1 부분 profile의 귀납 가정으로 다룬다. 완성된 1L 게임만을 기저로 두면 이 경우를 다룰 수 없다. 이 정리는 [catalog.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs)의 행 tuple 생성과 [inverse_projection.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/inverse_projection.rs)의 필요조건, [extended_inverse_catalog.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/extended_inverse_catalog.rs)의 같은 의미를 설명한다.

### H4. 실제 보드 식은 lock 수에 대한 별도 귀납으로 닫힌다

모든 합법 실행 prefix S에서

$$
B(S)=\Phi_{D(S)}U(S).
$$

기저에서 정규화된 초기 보드에는 full row가 없다. i를 더하면 새 full row는 정확히 D(S∪{i})∖D(S)다. 그 행의 마지막 미실행 contributor가 i였기 때문이다. H2의 삭제 합성을 쓰면 다음 보드 식을 얻는다. □

같은 S가 같은 occupancy와 frame을 정한다는 뜻이며, 그 S로 가는 모든 순서가 적법하다는 뜻은 아니다. 공급/관측 이력까지 같다는 주장도 아니다. **행 높이 H의 귀납과 실행 길이 |S|의 귀납을 분리**한 것이 5L 이상으로 확장되는 정확한 증명이다. 현재 `CompactedRowFrame`·Board64의 표현 한도나 제품 admission을 이 정리만으로 넓히지 않는다.

## 3. 기존 Build 선행 폐쇄의 정확한 일반화

### I1. 고정 표준 skeleton의 ILC 조건은 gap 삭제 조건이다

표준 connected tetromino의 점유 local row는 연속한다. 유효한 ILC skeleton Pᵢ의 점유 원래 행을 Uᵢ, 최저/최고 점유 행 사이의 비점유 행을 Gᵢ라 하자. 그러면

$$
\operatorname{ILC}(P_i,D)
\iff G_i\subseteq D\;\land\;U_i\cap D=\varnothing.
$$

**필요성.** 실제 lock 셀은 삭제 행에 있을 수 없다. 두 연속 shape row 사이에 생존 gap이 있으면 physical row 간격이 1보다 커진다.

**충분성.** 모든 점유 행이 생존하고 gap이 삭제되면 f_D(t_b)−f_D(t₀)=b다. x-pattern은 변하지 않으므로 원래 shape의 이동 결과와 정확히 같다. H1의 역함수로 원래 셀도 정확히 복원된다. □

pending i∉S에서는 Uᵢ∩D(S)=∅가 자동이다. 그렇지 않으면 i∈Cᵣ⊆S라는 모순이다. Gᵢ⊄F이면 i는 영원히 불가능하다. 그 외에는

$$
M_i=\bigcup_{r\in G_i}C_r,
\qquad \operatorname{ILC}(P_i,D(S))\iff M_i\subseteq S.
$$

### I2. ILC-only 폐쇄, DAG, 전체 순서의 존재는 동치다

모든 Gᵢ⊆F인 경우를 다룬다. 그렇지 않은 operation은 I1로 먼저 거절한다. A₀=∅, Aₖ₊₁=Aₖ∪{i:Mᵢ⊆Aₖ}라 하자. 다음은 동치다.

1. A∞=I.
2. j∈Mᵢ일 때 j→i를 둔 그래프가 비순환이다.
3. 모든 단계에서 ILC 조건을 만족하는 전체 operation 순서가 있다.

**1⇒3.** 각 반복에서 새로 ready가 된 operation을 나열한다. 필요한 선행자는 직전 반복에 이미 들어왔다. **3⇒2.** cycle에서 처음 실행한 operation은 아직 실행하지 않은 cycle 선행자를 요구하므로 모순이다. **2⇒1.** 정체된 남은 집합이 있다면 각 원소에 남은 선행자가 있다. 유한 집합에서 이를 계속 따라가면 반복 정점, 즉 cycle을 얻어 모순이다. □

따라서 ILC-only 언어는 정확히 이 poset의 linear extension이다. grounding·movement·공급까지 통과한 실제 언어라고 확대하지 않는다. 이 연산에는 2ⁿ subset 생성이 필요하지 않다.

기존 `dependency_relaxation_stalls`는 concrete realization requirement를 OR하는 callback으로 같은 단조 폐쇄를 계산한다. 표준 고정 skeleton의 같은 Gᵢ로 환원되는 경우 위 DAG 정리를 적용한다. 여러 서로 다른 placement/shape 대안을 한 operation에 묶으면 OR-of-AND이므로 **일반 closure를 단일 DAG로 바꾸지 않는다**. 각 대안의 필요 선행자 교집합만 취하면 안전한 약화일 뿐 모든 정체를 찾지 못한다.

현재 함수 자체는 full target F를 계산하지 않고 nonempty contributor가 모두 놓인 행을 삭제 가능으로 취급한다. 일반 재사용에는 실제 F를 명시적으로 결박하거나, F 밖의 낙관적 행을 requirement가 읽지 않는다는 불변식을 증명해야 한다. 그렇지 않으면 정확한 ILC 판정이 아닌 상한으로만 사용한다. `false`가 물리 가능성을 승인하는 값인 것도 아니다.

### I3. 부분 타일링에서 확정한 필수 cycle은 모든 completion을 거절한다

이미 선택한 footprint 집합 J에서 Pⱼ가 Gᵢ의 행에 기여하면 j→i를 확정할 수 있다. 모든 completion은 같은 셀 ownership을 유지하므로 이 간선을 포함한다. J의 확정 간선에 cycle이 있으면 모든 완성 그래프에도 cycle이 있고, I2에 의해 어느 completion도 실행할 수 없다. □

미확정 owner를 한 가지로 가정해서는 안 된다. 기존 [Build 회귀](../../crates/clearra-core-executor/src/backend/wasm_cpu/build_probability.rs)에는 T가 원래 1·3행을 차지하며 2행 삭제를 요구하고, Z가 0·2행을 차지하며 1행 삭제를 요구하는 실제 T/Z fixture가 있다. 이 상호 선행은 기존 구축 확률에서 이미 다루는 부류다. 그 fixture의 base와 공급을 빈 P7P4의 실제 발생 빈도로 바꿔 설명하지 않는다.

## 4. 프레임과 prefix를 열거하지 않는 받침 분석

### G1. 받침은 원래 행 interval의 명제로 쓸 수 있다

생존 target cell (x,y)와 lower cell (x,z), z<y가 물리적으로 바로 위아래일 필요충분조건은

$$
\operatorname{Adj}_D(z,y)
\iff z,y\notin D\;\land\;\{z+1,\ldots,y-1\}\subseteq D.
$$

target cell이 바닥에 닿는 조건은

$$
\operatorname{Floor}_D(y)
\iff y\notin D\;\land\;\{0,\ldots,y-1\}\subseteq D.
$$

증명은 f_D(y)−f_D(z)가 1과 같으려면 사이의 생존 행 수가 0이어야 한다는 계산이며, floor는 f_D(y)=0과 같다. □

O_S(x,z)를 그 원래 셀이 B₀ 또는 이미 실행한 owner의 셀이라는 명제로 두면, pending i의 정확한 grounding은

$$
\bigvee_{(x,y)\in P_i}\left[
\operatorname{Floor}_{D(S)}(y)\;\lor\;
\bigvee_{z<y}\bigl(\operatorname{Adj}_{D(S)}(z,y)\land O_S(x,z)\bigr)
\right].
$$

완전한 footprint가 충돌 없이 실현된다는 전제에서 한 칸 아래로 이동하면 막히는 조건과 동치다. i 자신의 셀은 아직 보드에 없으므로 supporter가 아니다. 받침은 AND가 아니라 **어느 하나의 바닥/셀**이다.

### G2. 양성 Horn 폐쇄의 거절은 false prune을 만들지 않는다

operation 완료 사실 pᵢ, row 삭제 사실 dᵣ, ILC 준비 aᵢ, 받침 가능 gᵢ를 둔다. 아래의 양성 규칙을 공유한다.

| 규칙 | 의미 |
| --- | --- |
| ∧ⱼ∈Cᵣ pⱼ → dᵣ, r∈F | 모든 contributor가 실행되면 행 삭제 |
| ∧ᵣ∈Gᵢ dᵣ → aᵢ | gap 삭제로 ILC 준비 |
| ∧₀≤ᵣ<ᵧ dᵣ → gᵢ | (x,y)∈Pᵢ의 floor 대안 |
| ∧𝓏<ᵣ<ᵧ dᵣ → gᵢ | 아래 (x,z)가 B₀의 셀인 대안 |
| pⱼ ∧ ∧𝓏<ᵣ<ᵧ dᵣ → gᵢ | 아래 셀 owner가 j≠i인 대안 |
| aᵢ ∧ gᵢ → pᵢ | ILC와 받침 필요조건의 결합 |

삭제될 수 없는 행을 요구하는 interval 규칙은 만들지 않는다. ILC에 여러 대안이 있으면 대안마다 aᵢ를 얻는 규칙을 남긴다. supporter 생존, 음성 리터럴, frame 사이의 양립성, movement를 제거하므로 **상한**이다. 서로 다른 frame에서 얻은 aᵢ와 gᵢ가 함께 남을 수 있다.

**정리.** 이 규칙의 least fixed point L에는 모든 실제 합법 prefix에서 실행한 pᵢ가 포함된다. 따라서 필수 pᵢ 하나라도 L에 없으면 완성은 불가능하다.

**증명.** 실제 lock 수로 귀납한다. 이전 실제 pⱼ가 L에 있으면 실제 삭제 행도 contributor 규칙으로 L에 있다. 다음 i의 ILC 대안이 aᵢ를 준다. 실제 grounding은 G1에 의해 floor 또는 실제 생존 lower cell 하나를 가지므로 대응하는 규칙이 gᵢ를 준다. 그러면 pᵢ도 L에 들어간다. 모든 실제 경로에 적용되므로 L 밖의 필수 operation은 어떤 경로에서도 실행할 수 없다. □

규칙은 고정 셀·owner·원래 행 interval에서 생성한다. 모든 D 또는 S를 열거하지 않으며 이 규칙군의 크기는 H와 n에 대해 다항식이다. 각 fact는 forward chaining에서 한 번만 새로 추가된다. 이것은 bit packing이 아니라 반복 frame/prefix 생성의 제거다. **모든 i∈I에 대해 pᵢ∈L이어도 물리 가능성의 증명이 아니다.** 규칙계의 고정점에 대한 완전성과 실제 이동의 완전성을 혼동하지 않는다.

### G3. ILC-only cycle보다 강한 금지 패턴

다음 두 원래 footprint를 고정한다.

$$
P_T=\{(0,0),(1,0),(2,0),(1,2)\},\qquad
P_L=\{(0,1),(1,1),(0,2),(0,3)\}.
$$

T는 표준 T를 원래 row 1로 벌린 ILC skeleton이고, L은 gap 없는 표준 L 회전이다. T를 놓으려면 L이 기여하는 row 1이 먼저 지워져야 한다. L의 bottom cells 아래는 T이며, 같은 column에서 더 높은 셀 아래는 T 또는 L 자신이다. L이 바닥으로 내려오려면 row 0이 지워져야 하는데 그 contributor에도 T가 있다. 따라서 T와 L 중 먼저 실행할 수 있는 것이 없다.

ILC만 보면 L→T의 국소 선행은 비순환이다. 받침을 결합해야 닫힌 불가능 집합이 드러난다. 관련 아래 셀 ownership이 이미 두 footprint로 고정됐으므로, 겹침 없는 모든 completion에서도 이 모순은 유지된다. 이것은 **셀로 제시한 수학적 금지 패턴**이며, P7P4 공급을 만족하는 완성 후보의 발생 여부/빈도나 실행 회귀를 새로 측정한 결과는 아니다.

### G4. gap 없는 전체 PC 타일링에는 받침 부정 검사가 불필요하다

전체 직사각형 PC target의 exact cover이고 모든 Pᵢ가 gap 없는 ordinary shape이면, ILC와 grounding만을 요구하는 전체 순서는 항상 존재한다.

**증명.** 남은 operation 중 최저 원래 셀의 행 m이 가장 낮은 i를 고른다. m 아래의 target 셀은 모두 B₀ 또는 이미 실행한 operation의 것이므로 그 행들은 가득 차 이미 삭제됐다. i의 최저 셀은 physical row 0으로 내려온다. i 자신의 점유 행은 아직 삭제되지 않았고 gap이 없으므로 ILC도 성립한다. disjointness로 충돌하지 않는다. 이를 반복하면 된다. □

이 정리는 entry/movement를 증명하지 않는다. 지붕이나 ordered kick 때문에 진입이 막힐 수 있다. 다만 이 부류에서 ILC+ground 부정 판정을 얻으려고 새 closure를 계산할 이유는 없다. 새로운 받침 선분류의 주 대상은 **gap·clear·support가 결합한 후보**다. 부분 Build target에는 아래 모든 행이 full이라는 전제가 없어 자동 적용하지 않는다.

검사 생략은 **후보 전체에 ILC+ground 순서가 하나도 없음을 찾는 prepass**에 한정한다. 각 prefix의 eᵢ(S)나 upper relation에서 grounding guard를 제거하는 근거는 아니다. 예를 들어 위아래 O를 쌓은 후보에는 합법 순서가 있지만, 빈 prefix에서 위 O부터 놓는 것은 여전히 불가능하다.

### G5. 부분 타일링의 닫힌 집합 증거

row/ILC/support helper를 전개하면 operation 규칙은 ∧ⱼ∈A pⱼ→pᵢ다. 예를 들어 cell supporter j를 쓰는 대안의 A는 Mᵢ∪(⋃𝓏<ᵣ<ᵧCᵣ)∪{j}다. K≠∅의 각 i를 만드는 **모든** 대안에 K의 선행자가 하나 이상 있으면 K에서 최초로 실행할 operation이 없어 불가능하다.

부분 타일링 σ를 버리려면 모든 completion T∈𝓕(σ)에 대해 이 닫힘을 증명해야 한다. 임의의 T를 고정하고 최초 operation 모순을 적용하면 보편 거절이 따른다. 알려지지 않은 owner나 가능한 외부 supporter를 생략하면 증명은 성립하지 않는다. G3은 관련 아래 ownership을 이미 고정했으므로 보편 조건을 만족한다. 일반 AND/OR 규칙의 이런 닫힌 집합은 단순한 pairwise SCC보다 풍부하다.

## 5. 모든 미노의 이동을 수식으로 표현하고 무관한 비교를 없애는 조건

### M1. Ordered kick을 포함한 Boolean 회로는 정확한 이동 관계다

실제 entry·sky·ceiling·이동 규칙을 완전히 보존하는 유한 pose template (V,E)를 고정한다. |V|=0이면 도달 가능한 pose는 없다. primitive guard γᵤᵥ(S)는 출발·도착의 완전한 셀 footprint와 경계를 검사한다. 회전의 j번째 후보는

$$
\operatorname{Free}_S(u)\land\operatorname{Free}_S(v_j)
\land\bigwedge_{k<j}\neg\operatorname{Free}_S(v_k)
$$

로 표현한다. 앞 후보가 보드에 따라 막힐 수 있으면 생략하지 않는다. 정적으로 불가능한 후보는 항상 false로 처리할 수 있다. translation은 대응하는 양 끝 Free 조건을 갖는다.

$$
R_0(v,S)=\operatorname{Seed}(v,S)\land\operatorname{Free}_S(v),
$$

$$
R_{k+1}(v,S)=R_k(v,S)\lor
\bigvee_{u\to v}\bigl(R_k(u,S)\land\gamma_{uv}(S)\bigr).
$$

**증명.** k=0은 유효 entry 자체다. 길이 k+1 이하 경로는 이미 k 이하이거나 마지막 edge 앞의 경로가 k 이하다. 역으로 각 항은 그런 경로를 준다. 경로 길이 귀납으로 Rₖ는 정확히 k 이하 도달성을 뜻한다. 유한 Boolean graph의 도달 경로는 cycle을 제거해 단순 경로로 만들 수 있으므로 R_|V|−1이면 충분하다. □

원래 점유, δᵣ(S)=[r∈F]∧∧ⱼ∈Cᵣ[j∈S], f_D를 써서 각 physical cell의 점유와 target 위치를 수식화할 수 있다. 모든 원래 셀의 생존 여부와 physical rank를 OR하면 exact board predicate가 된다. 따라서 위 식은 각 concrete S를 먼저 열거하지 않아도 정의된다. 같은 종류의 여러 target은 **하나의 symbolic S에서** R(v,S)를 공유한다. target 검사는 κ와 **완전한 4-cell footprint**가 Φ_D(Pᵢ)와 같은 pose의 존재 OR다. clipped mask 일치로 대체하지 않는다. H1의 역상 유일성 때문에 ILC witness와 이 full-footprint 이동 witness를 결합할 수 있다.

공유 DAG로 유지하는 recurrence gate 수는 primitive/row-projection 회로를 제외하고 O(|V|(|V|+|E|))다. 이를 분배 전개한 거대한 DNF로 만들 필요는 없다. 서로 다른 실행 단계 Sᵢ(t)에 대입하면 추가 공유 증명 없이는 최대 n개 회로 인스턴스가 필요하다. 이 표현 정리가 solver 비용이나 모든 target 분석이 한 번의 값 평가로 끝난다는 뜻은 아니다.

### M2. 상단과 높이의 경계

“H에서 도달 가능하면 H+1에서도 가능”은 참이 아니다. O 위에 좁은 구멍만 둔 지붕을 추가해도 진입이 막힐 수 있다. 도달성에는 H의 성공 귀납 대신 M1의 경로 귀납을 사용한다.

sky entry를 모델이 허용하는 모든 자유 pose로 정의했고 목표 anchor가 H_s보다 아래라 하자. 모든 단일 이동의 최대 하향 anchor 변화가 유한 Δ이면, 어떤 경로의 마지막 y≥H_s→y<H_s 이탈에서 출발 anchor는 H_s−1+Δ 이하다. 그 이후 suffix는 H_s 아래에 있다. 따라서 그 출발 pose를 유효 seed로 포함하고 suffix의 모든 primitive를 보존하는 유한 template이면 그 경로를 놓치지 않는다. 반대 방향은 template의 seed와 edge가 원래 계약에서 허용되면 즉시 성립한다. **실제 fixed spawn에 대해 sky seed가 과대근사일 뿐이면 음성 판정에만 안전하며 exact 양성으로 승격하지 않는다.** 이전 kick 후보의 충돌 판정도 ceiling 때문에 잘라내서는 안 된다.

층/영역을 나눠 재사용하는 별도 방법도 있다. V=V₁∪V₂, 경계 B=V₁∩V₂이고 B를 거치지 않는 두 내부 사이 edge가 없다고 하자. Tⱼ를 Vⱼ 내부의 B→B 도달 관계로 두면

$$
\operatorname{Reach}_{V_1\cup V_2}|_B=(T_1\cup T_2)^*.
$$

전체 경로를 경계 방문마다 자르면 한쪽 내부의 transfer로 분해되고, 반대로 각 transfer의 witness를 이어 붙이면 실제 경로다. □ 단순한 층별 성공 Boolean 하나로 대체할 수 없고, 출발/목표를 경계에 포함하며 crossing pose·earlier-kick read·경계 occupancy를 보존해야 한다. 새 strip engine을 구현했다는 주장이 아니다.

### M3. 여러 무관 변수를 함께 제거하려면 공동 조건이 필요하다

Γ를 i가 미실행인 합법 입력을 포함하는 분석 domain, x를 operation 실행 변수라고 하자. 남길 변수 집합 N에 대해 eᵢ가 N만의 함수로 잘 정의될 필요충분조건은

$$
\forall x,y\in\Gamma:\quad x|_N=y|_N\Longrightarrow e_i(x)=e_i(y).
$$

**증명.** 조건이 성립하면 각 projection 값에 어느 대표를 골라도 eᵢ 값이 같다. 반대로 N만의 함수가 있다면 같은 projection의 값은 같아야 한다. □

개별 toggle 검사를 각각 통과한 변수를 한꺼번에 지우는 것은 안전하지 않다. Γ={(0,0),(1,1)}, e=x₁이면 한 좌표만 다른 두 허용 상태가 없어 각 단독 toggle 검사는 공허하게 통과한다. 그러나 두 변수를 모두 지우면 0과 1의 차이를 잃는다. 최소 충분 집합도 {x₁}, {x₂}처럼 유일하지 않을 수 있다. 순차 제거에는 매번 quotient/domain을 갱신해야 한다.

싼 충분조건은 동일 frame·target·entry·규칙 아래 **모든 사용 primitive와 앞선 kick 후보의 read 영역**을 보존한다는 인증이다. 이 영역 밖 변수들의 동시 변화가 어느 read도 바꾸지 않으면 M1의 귀납으로 eᵢ 전체가 같다. 직접 이동 간섭이 없어도 δᵣ, support, target projection을 바꾸는 owner는 남겨야 한다. O의 무킥 계약과 다른 미노/킥 프로필의 계약은 별도로 증명하되 같은 공동 조건을 사용한다. 최대 단일 kick 반경만으로 전체 경로 또는 모든 미래 target의 무관성을 주장하지 않는다.

## 6. 순위 제약과 확률: 모든 prefix를 먼저 만들 필요는 없다

### R1. 전체 순서의 정확한 rank 표현

tᵢ∈{1,…,n}와 AllDifferent(t)를 두고 Sᵢ(t)={j:tⱼ<tᵢ}라 한다. 다음 식을 Φ_T(t)라 정의하면 전체 실행 존재성은 ∃t Φ_T(t)다.

$$
\Phi_T(t)=\operatorname{AllDifferent}(t)\land
\bigwedge_{i\in I}e_i(S_i(t))\land\operatorname{Terminal}(I).
$$

**증명.** 합법 operation word에서 tᵢ를 실행 순번으로 잡으면 각 prefix와 eᵢ가 일치한다. 반대로 서로 다른 1..n 순위는 permutation을 하나 정하고, 매 단계 eᵢ가 참이므로 H4와 M1의 계약 아래 실제 실행을 이어 붙일 수 있다. □ 전단사는 rank와 operation word 사이이며 movement/hold witness 사이의 전단사가 아니다.

행 r가 i 이전에 삭제됐다는 명제는

$$
\delta_r^{(i)}(t)=[r\in F]\land\bigwedge_{j\in C_r}(t_j<t_i).
$$

ILC 선행은 j∈Mᵢ마다 tⱼ<tᵢ이고, G1의 interval·ordered-kick 회로도 이 δ에 대입할 수 있다. 이로써 2ⁿ prefix 정점의 선생성 없이 정확한 유한 제약식을 얻는다. 제약식 풀이의 최악 비용이 다항식이라는 결론은 나오지 않는다.

### R2. 확률은 witness를 투영한 뒤 큐를 한 번씩 센다

유한한 원래 큐 universe Ω, 유한한 비음수 가중치 μ(q), 총질량 0<Z<∞를 고정한다. Supply(q,t,u)는 기존 hold·cursor·종료 계약에 맞는 공급 witness u의 명제다. 고정 타일링 T의 성공은

$$
\operatorname{Success}_T(q)
\iff\exists t,u:\ \Phi_T(t)\land\operatorname{Supply}(q,t,u).
$$

여러 후보의 oracle 성공은 ∃T를 추가한다. 확률은

$$
P=Z^{-1}\sum_{q\in\Omega}\mu(q)\,
\mathbf1[\exists T,t,u:\Phi_T(t)\land\operatorname{Supply}(q,t,u)].
$$

같은 q에 여러 순서·hold·타일링이 있어도 한 번만 센다. 전체 satisfying assignment (q,T,t,u)의 수를 그대로 세면 다른 문제다. 개별 큐 목록이 필요하지 않은 요청은 이 **projected weighted counting** 또는 같은 의미의 기존 공급 연산을 소비할 수 있다. 새 공급 엔진이 필수는 아니다.

같은 universe에서 정확한 Q⁻⊆Q*⊆Q⁺를 얻었다면 |Q⁻|=|Q⁺|만으로도 두 집합은 같다. 모든 μ(q)>0이면 질량 동등도 같은 결론을 준다. 0가중치 큐를 허용하면 질량 동등은 확률 값만 확정한다. 이 정리는 deduplicate된 원래 q의 집합에 적용하며 경로 수에는 적용하지 않는다. see-7의 서로 다른 최적 정책이 선택한 성공 집합은 자동으로 중첩되지 않는다.

## 7. Finesse 객체 없이 고정 타일링의 see-7을 보존하는 증명

### P1. 의존성 경로와 물리 실행의 lift/forget

pending i의 실제 lock footprint는 $L_i(S)=\Phi_{D(S)}(P_i)$로 정해진다. 실제 실행을 operation ID로 바꾸면 각 lock이 eᵢ의 증인이므로 같은 관계 경로를 얻는다. 반대로 true eᵢ(S)의 구체 실현과 이동 witness 하나를 S,T,rule,i만으로 정한 순서에 따라 선택한다. H4에 의해 다음 보드는 B(S∪{i})이고, 실행 길이로 반복하면 물리 실행을 얻는다. □

여러 orientation이 같은 kind/full footprint를 만들더라도 이번 Boolean 목표의 lock 이후 상태는 같다. witness를 미리 action 객체나 전체 입력 경로 DAG로 materialize할 필요가 없다. 출력에 witness가 필요하면 별도 재구성 계약을 만족해야 한다.

### P2. 관측 전에 operation을 확정하면 정책도 정확하다

관측 이력과 양립하는 큐 집합 Ω′, 공개된 S, hold/cursor/phase를 상태로 둔다. 같은 관측에서는 공급 선택과 i 선택이 같아야 한다. P1의 witness selector는 숨은 suffix를 보지 않으므로 관계 정책을 물리 정책으로 올려도 이 제약을 지킨다. 물리 정책을 ID로 내리는 방향도 새로운 정보를 만들지 않는다. 따라서 대응 정책은 각 원래 q의 성공 여부와 가중 성공 질량을 보존한다. □

관측 뒤 후속을 o별로 나누는 올바른 재귀는 정규화 전 질량 F에 대해

$$
F(s,\Omega')=\max_a\sum_o
F(T(s,a,o),\Omega'[a,o]).
$$

같은 관측에서 먼저 하나의 a를 고르고, reveal로 구별된 각 o에 이후 정책을 붙인다는 사실을 남은 단계 수로 귀납하면 된다. 공개 이전의 max를 sum 안으로 옮기면 숨은 정보를 사용한다. 큐마다 독립적으로 ∃t를 선택하는 R2는 oracle 의미이며 이 정책 제약을 대신하지 않는다.

현재 `StoreCurrentUseNext`처럼 공급 선택과 lock이 원자적인 계약에는 그 전이를 그대로 쓴다. 빈 hold 직후 preview를 보고 lock 위치를 선택하는 계약이면 `hold→reveal→i 선택/lock→reveal`로 phase를 나눈다. 한 lock 전 hold 횟수 제한까지 상태에 포함하면 같은 귀납이 성립한다. “7개”보다 공개 시점이 핵심이며 현재 제품 명세와 결박해야 한다.

### P3. 같은 multiset도 kind 결정화를 자동 정당화하지 않는다

다음은 물리 필드가 아니라 **조건부 operation 관계의 논리 반례**다. I={Iₐ,Iᵦ,Oₐ,Oᵦ,T,S,Z,J,L}이고 완성 경로를 다음 둘로만 둔다.

- A: Iₐ,Oₐ,T,S,Z,J,L,Iᵦ,Oᵦ
- B: Iᵦ,Oₐ,T,S,Z,J,L,Oᵦ,Iₐ

각 eᵢ(S)는 해당 두 경로의 prefix에서만 참으로 정의할 수 있다. 큐 `IOTSZJLIO`, `IOTSZJLOI`를 각각 질량 1/2, hold 없음으로 둔다. current를 포함한 첫 7개가 같으므로 첫 I의 concrete operation 선택은 공통이어야 한다. A는 첫 큐, B는 둘째 큐만 성공하므로 최적값은 1/2다. kind 후속을 합치면 첫 I 실행 후 8번째 reveal을 보고 과거 Iₐ/Iᵦ 선택을 바꾼 것처럼 값 1을 얻을 수 있다.

이 반례는 고정 multiset 조건만으로 arbitrary relation의 kind 결정화가 안전하다는 일반 명제를 부정한다. 이 구조가 실제 fixed-tiling physics에서 구현되는지 검증하지 않았으므로 현재 제품의 실제 필드 오답으로 선언하지 않는다. 안전한 연결은 operation별 child를 유지하거나 다음 절의 더 강한 동치를 증명한다.

### P4. 여러 타일링을 합칠 때의 commitment

각 타일링별 최적값의 max는 전체 적응 정책의 하한일 수 있고, 큐별 oracle 성공 합집합은 상한이다. see-7에서는 이미 실행한 선택과 양립하는 residual family를 유지해야 한다. 같은 공개 보드에서 algebraic event (κ(i),Φ_D(Pᵢ))가 같은 continuation을 결합하는 방법은 가능하며 finesse 객체가 필수는 아니다. 원래 frame이 다른 후보는 각 Φ를 적용하거나 그 동치를 증명한다. 서로 다른 footprint까지 합치려면 다음 절의 정책 합동을 증명하고 대표 선택을 **reveal 전에** 고정해야 한다.

operation commitment를 보존하는 완성경로 liveness prune도 Boolean 정책 값을 보존한다. 제거된 edge를 선택한 정책 branch는 어떤 큐에서도 완성할 수 없어 질량 0이며 실패 terminal로 바꿀 수 있다. 남은 action은 원래 action이므로 양방향 값이 같다. kind word 집합만 보존한 변환에는 이 증명을 적용하지 않는다. PC full terminal로 정리한 관계를 Build의 중간 terminal에 그대로 재사용하지 않는다.

## 8. 2ⁿ 상태를 실제로 줄이는 residual 몫

### Q1. 미래 관계의 동형은 oracle/see-7 정책 합동이다

prefix S의 남은 operation J_S=I∖S와 residual guard

$$
r_i^S(Y)=e_i(S\cup Y)\quad(Y\subseteq J_S\setminus\{i\})
$$

및 residual terminal τ_S를 정의한다. S,T 사이 전단사 φ:J_S→J_T가 종류, 모든 미래 guard, terminal을 보존한다고 하자.

$$
\kappa(i)=\kappa(\phi(i)),\quad
r_i^S(Y)=r_{\phi(i)}^T(\phi(Y)),\quad
\tau_S(Y)=\tau_T(\phi(Y)).
$$

같은 공급/관측 상태에서 두 residual은 정책 동치다. 지금 enabled 여부는 Y=∅에서 같다. i를 선택하면 guard가 i=true인 cofactor로 바뀌고 φ에서 i를 제거한 제한이 후속 동형이다. 같은 kind는 같은 공급 전이와 reveal 분할을 만든다. 남은 operation 수와 공급 phase의 유한 순서에 대한 귀납으로 모든 후속의 질량, 마지막 max와 합이 같아진다. □

정책을 concrete state에 올리는 φ 역상은 공개된 state로 결정해야 한다. 숨은 큐에 의존하면 안 된다. 최적값과 대응 정책의 성공 집합은 보존하지만, 기존 정준 tie가 고른 대표 출력까지 같아야 한다면 tie label/order도 보존해야 한다.

실제 남은 ID 집합 J_S를 key에 그대로 남기면 S를 복원하므로 전역 2ⁿ 문제가 사라지지 않는다. **미래 구조를 보존하는 재명명** 또는 검증된 residual 타입이 필요하다. 모든 Y를 열거해 동형을 검사하면 비용을 옮긴 것뿐이다. 기존 guard 식의 rewrite·구조 대칭이 인증서를 주는 경우부터 적용한다. 합칠 수 없는 state를 별개로 두는 것은 정확성 손실 없이 가속만 줄인다.

### Q2. 교환 가능한 operation class는 개수로 충분하다

operation을 비대칭 경계 B와 class C₁,…,Cₖ로 나누자. 각 class 안의 임의 순열 π가 B를 고정하고 종류, 모든 eᵢ, terminal을 보존하면

$$
(S\cap B,\ |S\cap C_1|,\ldots,|S\cap C_k|)
$$

가 정확한 signature다. 같은 signature의 S,T에서는 각 class의 실행/미실행 원소를 각각 대응시키는 π가 존재하고 Q1의 동형을 준다. 따라서 state 수 상한은

$$
\boxed{2^{|B|}\prod_c(|C_c|+1)}.
$$

같은 kind만으로 class를 만들지 않는다. support·clear·ordered-kick guard까지 순열 불변이어야 한다. 모든 순서가 실제로 자유로운 특수 경우에는 종류별 남은 개수만으로 충분하지만 그 자유성부터 증명해야 한다.

### Q3. 동일 component의 진행 상태 histogram

공유 boundary z와 공급 상태를 유지하면서 동일한 미래 관계를 가진 m개 component를 생각하자. 완료를 포함한 local residual 타입이 k개이고, 같은 타입의 전이 종류·후속 타입·boundary 효과가 같다. component index를 바꾸는 것이 전체 residual의 동형이며 boundary 변화에 따른 나머지 타입 갱신도 이 순열과 교환한다고 가정한다. 그러면 타입별 개수 hᵣ만으로 충분하다. 같은 타입 중 어느 concrete component를 진행해도 Q1로 대응하고, 후속 histogram과 공유 상태가 같다. 따라서

$$
\#\text{histograms}\le {m+k-1\choose k-1}.
$$

특히 같은 길이 ℓ의 독립 chain m개는 (ℓ+1)ᵐ indexed 상태 대신 ${m+\ell\choose\ell}$개 histogram을 사용한다. 공동 boundary가 다른 component의 해석을 바꾸면 그 deterministic 재분류도 상태 전이에 포함해야 한다. 단순히 component 확률을 곱하지 않는다. 하나의 supply·hold·관측 상태에서 매 단계 진행할 component를 선택해 모든 interleaving을 보존한다.

**기하 예.** 빈 10×4 전체 목표의 각 폭 2 column strip에 O 두 개를 쌓은 고정 타일링을 생각하자. O의 위치를 바꾸는 kick이 없고 완전한 sky entry/수직 drop을 허용하는 Boolean 계약에서는 각 strip의 아래 O가 위 O보다 먼저여야 하며 다른 strip은 그 수직 통로를 막지 않는다. 아래 두 full row의 clear 뒤에는 미실행 위 O가 바닥으로 내려온다.

먼저 증명된 reachable 불변식 Γ(S)=“모든 strip에서 위 O가 실행됐다면 아래 O도 실행됐다”로 제한한다. e′ᵢ(S)=Γ(S)∧eᵢ(S)∧Γ(S∪{i})로 두어도 모든 실제 경로를 보존한다. Γ 안에서는 다음 아래 O 또는 아래 O가 완료된 strip의 위 O가 정확히 실행 가능하므로 관계는 다섯 chain과 같다. Γ 밖의 물리적으로 실행할 수 없는 subset에 대해서까지 원래 eᵢ의 열 교환 불변성을 가정하지 않는다. 이 e′의 residual에 Q1/Q3를 적용하면

| 표현 | 상태 수 |
| --- | ---: |
| 모든 subset | 2¹⁰=1,024 |
| 다섯 길이 2 chain의 reachable prefix | 3⁵=243 |
| 0개/아래만/둘 다 실행한 strip 개수 | ${7\choose2}=21$ |

아래 full row의 clear 여부도 아직 0개인 strip 수가 0인지로 정해진다. 이는 clear 사건이 있어도 quotient가 가능하다는 예다. **O 10개 공급은 표준 7-bag P7P4 사례가 아니며 위 수치는 시간 측정이 아니다.**

이미 실행한 ID의 영향이 모든 미래 guard·terminal·공급/관측의 cofactor에 흡수됐다면 별도 bit를 지울 수 있다. 미실행 operation은 guard에서 안 읽혀도 공급과 완료 의무가 남으므로 개수/타입으로 보존한다. clear됐다는 이유만으로 원래 행 이력을 버리지 않는다. future rank/select와 gap 조건의 residual 동치를 증명해야 한다. 비대칭 타일링에서 class가 모두 singleton이면 위 상한은 2ⁿ보다 작아지지 않는다.

## 9. CEGIS로 후단 실패를 안전하게 앞단으로 옮기는 완전한 틀

### C1. 모든 Boolean 경로 불가능성은 닫힌 cut으로 인증할 수 있다

유한 정확 graph의 시작 s, accepting 집합 A를 고정한다. 성공 경로가 없을 필요충분조건은 s∈C, C∩A=∅이고 모든 outgoing edge에 대해 닫힌 C가 존재하는 것이다.

**충분성.** s에서 시작한 경로는 귀납적으로 C 안에 남아 A를 만나지 않는다. **필요성.** 성공 경로가 없으면 s에서 도달하는 모든 state를 C로 택한다. 시작을 포함하고 A를 포함하지 않으며 정의상 outgoing에 닫혀 있다. □

완성 목표가 I인 operation prefix graph에서는 ∅∈C, I∉C인 C에 대해 입력 x의 경계 식을

$$
N_C(x)=\bigwedge_{\substack{S\in C,\ i\notin S\\S\cup\{i\}\notin C}}
\neg e_i^x(S)
$$

로 두면

$$
\operatorname{Unbuildable}(x)
\iff\bigvee_{C:\varnothing\in C,\ I\notin C}N_C(x).
$$

공급 실패까지 다루려면 정확한 공급 product graph의 state와 terminal로 바꾼다. 이 정리는 ILC, support, frame 상관, ordered-kick, 공급 실패를 모두 포괄하는 **완전한 인증 형식**이다. 모든 cut을 생성하자는 알고리즘이 아니며 작은 식으로 항상 표현되거나 싸게 발견된다는 보장은 없다.

여기서 실패는 물리 실행 또는 명시된 공급 product의 완료 **경로가 없음**이다. P3처럼 각 큐에는 oracle 경로가 있지만 하나의 관측 정책으로 모두 성공하지 못하는 값 손실을 단순한 경로 cut으로 분류하지 않는다. see-7의 값에는 P2의 정보 상태와 아래 C4의 정책 상한을 쓴다.

### C2. 안전성, 포괄성, 부분 타일링 양화를 구분한다

학습한 가드 Γₖ가 Domain 아래에서 Γₖ⇒N_Cₖ를 만족하면 그 부류의 거절에는 false prune이 없다. 반례 질의는 Γₖ를 만족하지만 Cₖ 밖으로 나가는 정확한 boundary edge가 존재하는지다. 있으면 실패한 조건만 강화한다.

반면 실제 모든 실패를 학습 가드가 포괄했다는 주장은

$$
\operatorname{UNSAT}\left(
\operatorname{Domain}(x)\land\operatorname{Rejected}(x)
\land\neg\bigvee_k\Gamma_k(x)\right)
$$

를 필요로 한다. 몇 개의 fixture 성공이나 counterexample를 더 찾지 못했다는 사실은 이 증명을 대신하지 않는다. 부분 타일링 σ 전체를 거절하려면 **모든** completion T∈𝓕(σ)에 대해 거절 인증이 성립해야 한다. completion마다 다른 인증을 쓰는 것은 가능하지만 대표 하나의 실패로 대체할 수 없다.

### C3. 현재 앞단으로 옮길 수 있는 부류와 정확 잔여

| 부류 | 앞단의 인증 | 남는 의무 |
| --- | --- | --- |
| 기하 자체 불가 | 기존 면적·공급 수량·projection·full-parent·인증된 separator | 기존 연산 재사용; APDP 실제 활성 범위 구분 |
| ILC 선행 정체 | I1–I3의 기존 closure와 확정 cycle | F/owner/대안의 정확한 binding |
| support-event 정체 | G1–G3, G5의 양성 closure와 닫힌 K | 실제 후보 빈도·검사 비용은 미측정 |
| 양성 closure 통과, frame/order 불가 | 정확한 frame 상관을 보존한 cut | 낙관적 사실의 혼합을 양성으로 승인하지 않음 |
| 상한 완성은 가능, 이동 불가 | 모든 target 실현을 덮는 ordered-kick/entry cut | 하나의 pose 실패만으로 operation을 거절하지 않음 |
| 물리 순서 가능, 공급 불가 | operation 관계 × 정확 공급기의 cut | 큐별/전체 universe 양화와 중복 제거 |
| 증명 범위 밖·미완료 | Unknown | 정확 잔여로 보존; 확률 0으로 바꾸지 않음 |

가드에 우선순위를 주고 이미 분류한 부류를 빼면 서로 겹치지 않는 실패 장부를 만들 수 있다. 마지막 잔여를 정확 관계로 해결하면 전체 판정의 누락은 없다. 현재 작은 선분류 규칙만으로 실제 P7P4의 모든 실패를 수집·분류했다는 증거는 없다.

CEGIS의 후속 순서는 고정 SHA/규칙/frame/공급의 실제 reject 선택 → 필요한 owner/interval/경계 추출 → 매개변수 가드 일반화 → 모든 boundary 또는 외부 support 대안의 배제 증명 → ∀completion lift다. 좌우 반사·평행이동·높이 확장은 형상뿐 아니라 ordered kick와 entry 의미도 보존할 때만 허용한다. 증명 생성의 비용이 원래 관계 생성 비용을 넘지 않도록 기존 식과 내부 연산의 결과를 먼저 재사용한다.

### C4. see-7의 값은 Bellman 상한으로 인증한다

P2의 정보 상태 graph에서 z=(s,Ω′), action 뒤 관측 후속을 z[a,o]라 하자. 남은 lock 수와 유한 hold/reveal phase의 horizon을 상태에 포함하여 모든 전이에 감소하는 자연수 rank h(z)가 있다고 가정한다. 유한 graph라는 사실만으로 이 역방향 귀납을 대신하지 않는다. 각 후속 Ω′는 원래 큐 질량을 유지한 서로 겹치지 않는 부분집합이다. 종료 상태에서는 정확한 성공 질량을 m(z)로 둔다. 모든 terminal에서 U(z)≥m(z)이고 모든 비종료 상태에서

$$
U(z)\ge\max_a\sum_o U(z[a,o])
$$

이면 정확한 최적 성공 질량 F(z)≤U(z)다. terminal에서 자명하고, 모든 후속에서 성립한다고 가정하면 P2의 max/sum이 부등식을 보존하므로 역방향 귀납이 닫힌다. action이 없으면 실패 terminal로 처리한다. 반대로 terminal에서 L(z)≤m(z)이고 각 비종료 상태에서 공개된 z만으로 고른 action a_z가 L(z)≤Σ_o L(z[a_z,o])를 만족하면 그 정책의 실제 질량은 L(z) 이상이다. 역시 같은 귀납이며, a_z가 숨은 큐에 의존하면 유효하지 않다. □

따라서 root의 L=U이면 **최적값**을 확정할 수 있다. 값이 같다는 이유만으로 서로 다른 최적 정책의 성공 큐 집합이나 정준 출력을 같다고 하지 않는다. 완전성은 유한 graph의 정확한 F를 U로, 최적 action과 F를 L로 택하면 양쪽 인증이 존재한다는 의미다. 작은 인증을 항상 싸게 구한다는 결론은 없다. 정책 값의 부족을 물리적으로 가능한 후보 전체의 불가능성으로 바꿔 prune하지 않는다. CEGIS에서 이 부류를 일반화하려면 선택 가능한 모든 action과 reveal 분할에 대한 Bellman 부등식을 보존해야 한다.

## 10. 완료한 증명과 남은 적용 작업

### 10.1 완료 범위

| 의무 | 이번에 완료한 내용 | 이 증명에서 나오지 않는 주장 |
| --- | --- | --- |
| 1L→일반 높이 | H1–H3 행 전단사·삭제 합성·tuple 열거, H4 실행 귀납 | 모든 5L 이상 제품 실행/표현 범위의 인증 |
| 기존 선행 관계 재사용 | I1–I3 ILC 조건·closure/DAG·부분 cycle의 보편 거절 | 일반 OR 대안을 단일 DAG로 대체 |
| 받침 조기 분류 | G1–G5 exact interval, 안전한 Horn 거절, 금지 패턴, 불필요 검사 부류 | 모든 실제 이동 실패의 저비용 사전 분류 |
| 이동의 수학 표현 | M1–M3 symbolic path 귀납, 경계 합성, 공동 무관성 | 인증되지 않은 ceiling/킥에도 exact라고 주장 |
| prefix 선생성 제거 | R1 rank 표현, R2 witness 투영 확률 | 모든 입력의 다항 시간 풀이 |
| see-7 | P1–P4 operation 선택의 정책 보존과 관측 재귀 | kind 합집합만으로 같은 정책을 보존 |
| 실제 state 축약 | Q1–Q3 residual 동형·개수·histogram | 모든 타일링에 작은 대칭이 존재 |
| CEGIS 완전 형식 | C1–C3 경로 cut 필요충분성, C4 정책 값 상하한, 안전성/포괄성 의무 | 실제 P7P4 실패 전체를 이미 계측·학습 |

높이 독립적인 수학 증명을 완료했다는 말과 구현이 그 전제를 모든 입력에서 만족한다는 말은 다르다. 제품의 5L 이상 활성화, 새로운 quotient/rank/Horn 적용, host별 성능은 후속 검증이다. 이번에는 제품·테스트 코드를 변경하지 않았다.

### 10.2 이후 적용의 작은 작업 단위

아래 항목은 **미착수 적용 작업**이며 이번 커밋에서 구현하지 않는다. 기존 파일의 실제 내부 연산과 파생 결과를 공유하고, 항목마다 새 파일·새 crate·새 엔진을 만들지 않는다. ILC/APDP 트랙은 반복 연산 제거에 한정하며 bit packing·컨테이너 교체·cache-line 등의 미세 최적화를 성과로 삼지 않는다. Rust/SRP는 작업·검토 규약이며 줄 수·marker·파일 수 정적 gate를 추가하지 않는다.

| 작업 | 선행 작업 | 적용 및 검증 단위 |
| --- | --- | --- |
| A | 없음 | F/owner/requirement와 operation/terminal/entry 계약 고정 |
| B | A | 기존 ILC closure 결과를 Build/PC 소비자에 공유 |
| C | A | 부분 타일링의 확정 cycle 증거 공유 |
| D | A | original-row support 규칙과 G4의 후보 전체 prepass 생략 조건 |
| E | C, D | 닫힌 집합의 ∀completion 인증 및 CEGIS 반례 의무 |
| F | A | ordered-kick guard 회로와 공동 무관성 인증 |
| G | B, D, F | rank 제약의 정확한 생성과 기존 공급 witness 투영 |
| H | A | guard rewrite·대칭으로 residual 동형 인증 |
| I | A | operation별 child를 보존하는 see-7 adapter와 reveal 계약 |
| J | H, I | quotient 정책 및 여러 타일링의 commitment 결합 |
| K | G, J | 5L 이상 범위별 의미 검증과 실제 host 성능 채택 |

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/realization_feasibility.rs`, 구현 내용: A/B의 기존 폐쇄에 실제 full-row/owner 계약과 대안 의미를 결박하고 필요조건·음성 증거를 같은 ILC 소비자들이 재사용하도록 연결한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs`, 구현 내용: A/D의 기존 gap·occupied-row·incidence 결과에서 floor/support interval 규칙을 유도하는 공유 view를 설계하고 raw realization 재계산을 피한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs`, 구현 내용: C의 확정 contributor/ILC 선행을 타일링 선택과 rollback에 결박하여 completion마다 같은 cycle을 재발견하는 연산을 제거한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/realization_feasibility.rs`, 구현 내용: D의 양성 support 폐쇄와 gap 없는 full-PC의 후보 전체 prepass 생략을 기존 owner에 통합하며, 각 prefix의 grounding guard는 보존하고 물리 양성 승인과 구분한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_domain.rs`, 구현 내용: E의 모든 completion 대안을 보존하는 거절 인증을 기존 APDP/domain 계약과 연결하며 빈 P7P4의 minimum 경로와 활성 APDP 실험을 구분한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_target.rs`, 구현 내용: F의 complete pose/ordered-kick 조건을 공유 식으로 노출하고 모든 삭제 변수의 공동 무관성 및 frame 변화 경계를 인증한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_prefix_orders.rs`, 구현 내용: G의 rank 표현이 기존 exact operation 경로와 동치인 범위를 연결하여 모든 prefix의 선생성을 필요조건에서 제거한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/coverage_product.rs`, 구현 내용: G의 공급 계산에서 원래 큐를 witness 존재성으로 투영해 중복 없이 질량을 구하며 이미 있는 hold/bag 연산을 재사용한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_prefix_orders.rs`, 구현 내용: H의 인증된 residual 재명명·class count·component histogram을 직접 생성하고 전체 ID 집합으로 S를 다시 보관하는 우회를 피한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy.rs`, 구현 내용: I의 operation별 후속 commitment와 hold/reveal phase를 기존 evaluator에서 보존하고 kind 언어 동치와 정책 합동을 구분한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/extended_pc_observation.rs`, 구현 내용: J의 고정 타일링 관계 또는 같은 물리 event의 residual family를 관측 소비자에 연결하며 현재 지원 범위와 admission을 명시한다.
- [ ] 수정 파일: `docs/v0.9.0-placement-dependency-rust-release-plan.md`, 구현 내용: K의 범위별 exact 동치, 반례, 회피한 prefix/target/ILC 연산 수, 인증 비용, peak memory, 동일 요청의 host별 성능을 실제 실행 증거로 기록한 뒤 채택을 판단한다.

B/C/D/F/H/I는 A의 의미 계약이 정해지면 독립 증명·작업 공간에서 병렬로 진행할 수 있다. 같은 hot file의 통합은 한 소유자가 순서대로 한다. E는 C/D의 증거, G는 exact guard, J는 정책 합동을 기다린다. 큰 실행은 의미 검증이 끝난 변경만 비교한다. 광범위한 Rust 전환과 host 통합은 기존 제품 장부에서 계속 추적하되 이 수학 작업에 damage/spin 적용이나 강제 객체지향 계층을 다시 추가하지 않는다.

## 11. 근거와 증거의 경계

소스 사실은 위 고정 SHA의 [ILC 행 프레임](../../crates/clearra-core-executor/src/row_frame.rs), [catalog](../../crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs), [Build 폐쇄](../../crates/clearra-core-executor/src/backend/wasm_cpu/realization_feasibility.rs), [geometry](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs), [prefix 관계](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_prefix_orders.rs), [관측 evaluator](../../crates/clearra-core-executor/src/backend/wasm_cpu/queue_observation_policy.rs), [지원 계약](../../crates/clearra-problem/src/search_problem.rs)과 대조했다. Build의 테스트 연결과 일반 제품 활성 범위를 구분했다. 식·반례·상태 수는 본문의 증명으로 도출한 값이며 새 실행 측정값이 아니다.

이 문서는 실제 소스를 바탕으로 검토한 증명을 기록한다. 수학적 완결성은 각 정리에 열거한 전제에 대한 것이고, 기계 검증·후속 구현의 정확성·배포 게이트 통과는 별도 증거가 필요하다. 검증된 빠른 인증과 정확 잔여를 함께 유지하면 아직 일반화하지 못한 부류가 있어도 원래 해법과 확률을 잃지 않는다.

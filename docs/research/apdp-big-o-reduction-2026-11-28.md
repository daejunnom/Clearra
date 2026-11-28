# APDP의 계산 복잡도 감소 연구: 정적 부모 도메인과 강제 부분도형

- 작성일: 2026-11-28
- 연구 브랜치: research/big-o-reduction
- 소스 분석 기준: main / 8ac5cc548488a740edd82eb02268f8b4faf8d60f
- 상태: **소스 감사 및 수학적 설계 제안. 구현·실행·성능 검증 미완료.**

## 1. 목적 및 경계

Clearra의 Big O 감소 연구 중 APDP(Arm-Pair Domain Propagation) 자체를 대상으로
한다. 사용자가 제시한 전제는 ILC(inverse lock-clear) 완료 후 줄 지우기가 없는
필드에 APDP를 적용하고, 가능한 모든 기하학적 미노 배치를 찾은 뒤 실제
배치 의존성을 확인한다는 것이다. 체커보드·4색 패리티는 이전 적용에서
성능 악화를 유발했다고 보고되었다. 이번 연구에서는 패리티를 새로 적용하거나
강화하지 않는다. 해당 성능 보고의 원시 계측 로그는 이번 작업에서 재검증하지
않았으며, 기존 코드의 additive policy 설정을 변경한다는 뜻도 아니다.

목표는 단순히 새로운 필터를 늘리는 것이 아니다. APDP의 중복 계산과 보조
자료구조를 제거하고, 동일한 기하 해법 집합을 유지하면서 불가능한 부모
조합을 더 일찍 제거할 수 있는 수학적 구조를 찾는다. 국소 연산의 Big O 감소,
탐색 방문 상태 수 감소, 전체 최악 복잡도 감소를 각각 구별한다.

APDP는 Geometry 내부의 exact support propagation이다. ILC 실현 생성,
Skeleton identity, Exact Cover의 완전성, 실제 배치 순서/도달성, SRS/SRS+
및 다른 kick profile, 큐/hold, coverage, 최종 결과 계약을 변경하지 않는다.
줄을 삽입하여 복원한 canonical frame에서 움직이는 줄이 없다는 것과
그 frame의 모든 Skeleton이 물리적으로 연결된 정적 테트로미노라는 것은
별개의 명제다. 아래 직접 부모 생성 정리는 후자까지 증명된 영역에만 적용한다.

모든 최적화는 작은 미노 수/특정 큐/특정 줄 수에 대한 전용 알고리즘이나
대형 상태별 룩업 테이블이 아닌 일반적인 구조와 증명 조건을 대상으로 한다.
현재 작은 입력에 별도 경로가 존재한다는 사실은 감사 결과일 뿐 새 분기
정책을 도입할 근거가 아니다.

## 2. 소스 감사: 현재 APDP의 실제 역할

아래 링크는 분석 시점의 소스에 고정되어 있다.

- [geometry_apdp.rs](https://github.com/daejunnom/Clearra/blob/8ac5cc548488a740edd82eb02268f8b4faf8d60f/crates/clearra-core-executor/src/backend/wasm_cpu/geometry_apdp.rs):
  각 Skeleton row에서 연결된 3-cell partial을 생성하고, 유효한 두 partial의
  조합을 열거한다. row당 최대 12개의 (partial,row ID) 레코드를 생성한 뒤
  정렬/dedup하여 ExactArmPairIndex를 구성한다.
- [geometry_domain.rs:267–489](https://github.com/daejunnom/Clearra/blob/8ac5cc548488a740edd82eb02268f8b4faf8d60f/crates/clearra-core-executor/src/backend/wasm_cpu/geometry_domain.rs#L267-L489):
  CellSupport의 count, piece mask, common_owner_cells로 SameTile 그룹을
  구성하고, 정확한 부모 수로 pivot을 선택한다. APDP 검사는 그 후에 수행한다.
- [geometry_domain.rs:743–794](https://github.com/daejunnom/Clearra/blob/8ac5cc548488a740edd82eb02268f8b4faf8d60f/crates/clearra-core-executor/src/backend/wasm_cpu/geometry_domain.rs#L743-L794):
  exact_parent_rows와 apdp_domain_is_complete는 같은 remaining 및
  feasible_piece_mask를 대상으로 부모 incidence를 검사한다.
- [extended_geometry_domain.rs:29–131](https://github.com/daejunnom/Clearra/blob/8ac5cc548488a740edd82eb02268f8b4faf8d60f/crates/clearra-core-executor/src/backend/wasm_cpu/extended_geometry_domain.rs#L29-L131):
  ExtendedArmPairIndex의 BTreeMap 기반 구축 및 검색.
- [extended_geometry_domain.rs:198–287](https://github.com/daejunnom/Clearra/blob/8ac5cc548488a740edd82eb02268f8b4faf8d60f/crates/clearra-core-executor/src/backend/wasm_cpu/extended_geometry_domain.rs#L198-L287),
  [row_allowed 및 completeness](https://github.com/daejunnom/Clearra/blob/8ac5cc548488a740edd82eb02268f8b4faf8d60f/crates/clearra-core-executor/src/backend/wasm_cpu/extended_geometry_domain.rs#L364-L451):
  그룹 부모 수를 계산하고 모든 유효한 부모가 static exact/APDP 지원 조건을
  만족하면 APDP required mask를 기록하며, 실제 row 검사에서 재확인한다.
- [catalog.rs:926–936](https://github.com/daejunnom/Clearra/blob/8ac5cc548488a740edd82eb02268f8b4faf8d60f/crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs#L926-L936),
  [extended_inverse_catalog.rs:425–433](https://github.com/daejunnom/Clearra/blob/8ac5cc548488a740edd82eb02268f8b4faf8d60f/crates/clearra-core-executor/src/backend/wasm_cpu/extended_inverse_catalog.rs#L425-L433):
  static exact 검사는 support flag 및 모든 realization의 required_deleted_rows
  == 0을 요구한다.
- [geometry.rs:1299–1373](https://github.com/daejunnom/Clearra/blob/8ac5cc548488a740edd82eb02268f8b4faf8d60f/crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs#L1299-L1373):
  compact compiler는 조건에 따라 advanced domain과 minimum domain을 선택한다.
  minimum domain에는 같은 APDP 검사 블록이 없다. 증분 CellSupport 계산과
  SameTile parent reuse는 기존 local-search-ab/test 실험에 존재한다.
- [extended_geometry_dense.rs:464–503](https://github.com/daejunnom/Clearra/blob/8ac5cc548488a740edd82eb02268f8b4faf8d60f/crates/clearra-core-executor/src/backend/wasm_cpu/extended_geometry_dense.rs#L464-L503):
  Extended domain 결과의 pivot incidence를 다시 확인하여 support_scratch에
  row ID를 기록한다.

기하 해법의 기본 단위는 full PlacementSkeleton row이다. ArmPair는 지원
증명이며 독립 해법 row가 아니다. SameTile 증명 없이 시각적으로 발견한
Arm/Elbow를 강제할 수 없다. T의 복수 분해도 새로운 배치 identity가 아니다.

### 2.1. 현재 APDP 후속 검사의 항등성

고정된 노드에서 E를 잔여 셀, M을 허용 piece mask, G를 이미 선택한
3-cell SameTile pivot이라 한다. 정확한 부모 집합은

$$
D(G)=\{r\in R : G\subseteq P_r\subseteq E,\ \mathrm{kind}(r)\in M\}.
$$

여기서 P_r는 row r의 네 셀이다. A(G)는 APDP index에 G의 부모로 기록된
row의 집합, S는 static exact 조건을 만족하는 row의 집합이다.

현재 apdp_domain_is_complete(G)는 다음 조건에서만 성공한다.

$$
D(G)\ne\varnothing,\qquad D(G)\subseteq A(G)\cap S.
$$

그러면 후속 APDP 필터의 결과는 항등이다.

$$
D'(G)=D(G)\cap A(G)=D(G).
$$

따라서 관찰한 compact 코드에서 completeness가 성공한 후
exact_parent_rows(..., true)의 count와 piece mask는 같은 입력으로 앞서
산출한 값과 같아야 한다. APDP 적용 조건은 3-cell pivot이므로 1-cell
bumper pivot으로 바뀐 경우도 이 경로에 들어오지 않는다. 정확한 동일
부모 도메인이라는 전제 아래 apdp_pivot_tightened 및 추가 empty 판정은
도달할 수 없다. Extended의 후속 row_supports도 이미 통과한 조건을
재검사한다. 일반 SameTile의 실제 가지치기와 이 후속 APDP 블록의
추가 가지치기를 혼동하지 않는다.

**증명 범위:** 위 두 Rust 경로에서 관찰된 동일 도메인 검사에 대한 소스
수준의 결론이다. 모든 브랜치, C 경로, 제품별 연결, 별도 proof consumer,
cache/serialization 계약까지 조사했다는 뜻이 아니다. 인덱스를 제거하기
전에 전체 소비자와 identity_digest, resource admission, 진단/증명 의미를
감사해야 한다. 잘못된 catalog 또는 요약 재사용으로 집합이 달라지는 경우는
새로운 가지치기의 성공이 아니라 정확성 회귀다.

### 2.2. 패리티와의 관계 (배경, 실행 제안 아님)

임의의 고정 셀 가중치 w와 sigma(X) = sum(w(c), c in X)에 대해, 두
3-cell partial A,B의 합집합이 P라면 포함배제에 의해

$$
\sigma(P)=\sigma(A)+\sigma(B)-\sigma(A\cap B).
$$

체커보드나 4색 signature는 full occupancy의 선형 투영에 해당한다.
인접한 두 공유 셀의 체커보드 차이는 정적 checker coloring에서 0이지만,
일반적인 공유 영역이나 줄 삭제에 의해 변형된 coloring에 그 가정을 적용할
수 없다. 정적 배치 정보가 패리티를 내포한다는 사실이 국소 APDP가 모든
전역 패리티 제약을 전파한다는 의미는 아니다.

기존 [APDP/M4/checker 연구](apdp-m4-checker-joint-domain-2026-09-12.md)는
역사적 자료로만 참고한다. 이번 연구에서 패리티, 색상 위상, additive DP,
결합 캐시 또는 패리티 성능 개선은 구현 범위에서 제외한다.

## 3. 정적 부모의 직접 완성 정리

### 3.1. 네 번째 칸의 상한

A가 격자에서 4-neighbor 연결된 세 셀(직선 Arm 또는 꺾인 Elbow)이고,
P가 이를 포함하는 정상적인 연결 4-cell tetromino라 하자.

$$
P=A\cup\{v\},\qquad v\in\partial A=N_4(A)\setminus A.
$$

반대로 A에 인접한 외부 셀 v를 추가하면 연결된 네 셀 도형이 된다.
필드 경계, 초기 블록, 잔여 필드, 허용 piece family 및 실제 catalog
row identity까지 통과한 것만 유효한 부모다.

- 직선 Arm: 외부 이웃은 최대 8개.
- Elbow: 외부 이웃은 최대 7개(오목한 부분에서 두 경계 간선의 끝점이 중복).

기하학적으로 서로 다른 부모 셀 마스크는 최대 8개다. 이 상한이 실제
catalog row ID의 개수 상한이 되려면 동일 geometry/piece의 canonical
row 규약을 확인해야 한다. 복수 realization과 기타 identity는 소실시키지
않는다. 경계가 있는 필드에서는 후보가 더 줄 수 있다.

### 3.2. 두 번째 연결 partial의 존재 증명

A의 연결 그래프에 v를 붙이는 간선을 하나 선택해 P의 spanning tree를
만든다. v는 그 트리의 잎이다. 정점이 4개인 트리에는 잎이 적어도 두
개이므로 v가 아닌 잎 u가 있다. u를 제거하면 나머지 세 정점은 연결되어
있다. B = P \ {u}라 하면 B는 Arm 또는 Elbow이며

$$
A\cup B=P,\qquad |A\cap B|=2.
$$

따라서 연결된 세 셀을 포함하는 모든 정상적인 테트로미노에는 반드시
그 세 셀을 포함한 ArmPair 분해가 존재한다. ArmPair 존재 검사를 별도로
반복할 이유가 없다. 이 정리는 I, O, T 및 모든 방향에 적용된다.

불연속적인 lifted Skeleton에 동일한 기하학적 상한을 강제해서는 안 된다.
정적 완성 정리의 전제가 증명되지 않을 때는 실제 catalog full-row
incidence를 사용한다. completeness 확인이 실패했다는 이유만으로 부모를
제거하지 않는다.

### 3.3. 인덱스 대체 가설

ArmPair 목록 대신 증명된 부분도형의 네 번째 칸을 bit mask 또는
기존 incidence에서 직접 구한다. 유효한 catalog row ID 및 realization을
정확히 복원할 수 없으면 일반 SameTile incidence로 처리한다.

정적 row에 대해 중복 APDP 후속 검사를 제거하는 일과, 전체 APDP
보조 인덱스의 수명/identity 계약을 변경하는 일을 별도 실험으로 분리한다.
새 대형 lookup table을 만들거나 모든 필드/큐 조합을 캐시하지 않는다.

## 4. APDP v2 후보: 강제 부분도형 도메인의 폐쇄

APDP v2는 연구용 가칭이다. ArmPair를 새 해법 단위로 만드는 것이 아니라,
이미 증명된 SameTile 관계로 기존 full-row 도메인을 강화하는 것이다.

### 4.1. 모든 관련 부모에 대한 SameTile 폐쇄

SameTile 그룹 G가 모든 유효한 completion에서 반드시 한 미노에
속한다는 증명이 있으면, 모든 사용 가능한 배치 P는 다음을 만족해야 한다.

$$
P\cap G=\varnothing\quad\lor\quad G\subseteq P.
$$

P가 G의 진부분집합만 점유하면, 해당 셀이 이미 같은 부모에 속해야 한다는
가정과 모순이다. 따라서 그런 row는 해당 residual state에서 제거해도 안전하다.
그 결과 다른 셀의 부모 집합이 축소되고 새로운 강제 관계가 나타날 수 있다.

제안하는 단조 전파 루프:

1. 실제 active full-row domain에서 셀별 부모 교집합과 support count를 구한다.
2. 같은 미노 소유가 증명된 셀을 그룹으로 결합한다.
3. 그룹 일부만 점유하는 row를 해당 상태에서 제거한다.
4. 영향을 받은 셀의 support와 교집합만 갱신한다.
5. 새 SameTile 그룹 또는 강제 row가 나타나면 반복한다.
6. 그룹 크기 > 4 또는 필요한 셀의 정확한 부모 집합이 공집합일 경우
   해당 residual state는 geometry completion이 없다. 다른 clear-state나
   요청 전체의 후보 삭제 증명으로 승격하지 않는다.

각 삭제는 유효한 completion을 보존하므로 반복 후에도 동일 해법 집합을
보존한다. 삭제는 단조이고 유한한 row 집합에 대해 종료한다. 다만
closure를 통과했다고 해법 존재를 보장하지 않는다.

현재 두 Rust domain 경로에서는 SameTile 그룹 및 pivot을 결정하는 코드는
확인했으나, 이와 같은 모든 영향 row의 삭제와 반복 고정점 유지가 해당
구간에 연결되어 있지는 않다. 다른 경로와 기존 propagator의 기능 중복을
추가 감사한 뒤 구현한다.

### 4.2. 강제 배치의 일괄 처리

D(G)의 원소가 하나이면 그 row는 모든 completion에 포함된다.
여러 강제 row가 존재하면 geometry 분기 없이 일괄 적용할 수 있다.

- 같은 row ID는 중복 확정하지 않는다.
- 서로 겹치거나 supply target을 모두 위반하면 정확한 모순이다.
- 남은 셀 및 사용한 미노 수, component identity, residual memo identity를
  일관되게 갱신한다.
- 모든 강제 row 및 realization을 완전한 해법 family와 이후 의존성
  확인에 전달한다. 강제 row 선택 순서가 실제 배치 순서인 것은 아니다.
- 기존 단일 후보 pivot과 비교할 때 방문할 중간 unary state를 축약하는
  효과이며, 실제 가능한 TilingVariant 수를 줄이는 것은 아니다.

### 4.3. 증분 도메인 유지

잔여 셀 E가 축소되거나 feasible piece mask가 감소할 때 이전 support
결과 중 영향받은 셀/row만 갱신한다. 기존 DomainSupportCache와
SameTileParentMode::Reuse를 우선 검토한다. 이 기능은 이미 실험적 코드로
존재하므로 새 구조로 무조건 재작성하지 않는다.

불변 catalog와 incidence는 워커가 공유하고, 가변 active domain은
워커/DFS 상태의 소유권 아래에서 작은 scratch/undo trail로 처리한다.
형제 branch 복원, piece availability 변경, cache key 동일성,
자원 부족 fallback을 명시적으로 검증한다. 역방향 회복 때 stale한
부모 교집합을 재사용하면 안 된다.

고정점을 매번 전체 row scan으로 구현하면 오히려 느려질 수 있다.
단조 구간에서 각 row 삭제 이벤트와 영향을 받은 incidence만 처리하는
방향이 후보이며, 셀별 교집합의 갱신 비용까지 포함한 엄밀한 amortized
bound는 구현 설계 및 검산의 후속 의무다.

### 4.4. 3-cell 그룹 사이의 경합: 이분 매칭

서로 다른, 이미 증명된 3-cell SameTile 그룹 G_i는 각각 정확히 한 셀을
더 필요로 한다. 그 추가 셀은 다른 SameTile 그룹의 내부를 침범할 수
없으며, 두 그룹이 동일한 추가 셀을 함께 점유할 수 없다.

왼쪽 정점 U는 그룹, 오른쪽 정점 V는 추가 셀, 간선은 실제 유효 부모
row가 있는 (G_i,v) 관계로 정의한다. 격자상의 정상 정적 부모만 포함하거나,
모든 가능한 일반 부모를 빠짐없이 포함한 그래프여야 한다.

$$
\forall X\subseteq U:\quad |N(X)|\ge |X|.
$$

Hall 조건 또는 왼쪽을 포화하는 매칭이 존재하지 않으면 해당 residual
state의 geometry completion은 불가능하다. 모든 매칭을 구성할 필요 없이
경합 성분의 최대 매칭으로 판정할 수 있다. 예: 세 그룹의 모든 후보가
같은 두 셀에만 집중된 경우 각 부모 도메인은 비어 있지 않아도 불가능하다.
이 예시는 추상적인 그래프 예이며 실제 Clearra 필드 strict witness는
향후 생성·검증한다.

정상적인 정적 그룹 g개에 대해 간선 수 <= 8g이고, 후보 오른쪽 정점
역시 <= 8g이다. 그래프 구축과 row ID 복원을 별도로 계산했을 때,
Hopcroft–Karp의 매칭 계산은 O(g^(3/2)) 시간 및 O(g) 공간 상한이다.
기존 미노 종류별 Hall bound와 다른 제약(셀 점유 경합)이다.

모든 그룹을 포화하는 매칭이 존재해도, 미노 inventory 및 나머지 셀의
tiling, realization/의존성까지 만족한다는 뜻은 아니다. 영역이 전부
증명된 그룹과 그 추가 셀로 구성되어 있는 경우에만 순수 기하의
일대일 배정 문제로 완전히 환원될 수 있다. 정확한 공급을 함께 만족하는
문제는 별도의 조건을 보존해야 한다.

매칭은 겹치는 후보가 발생한 구조적 경합 성분에 한해 실험한다.
모든 노드에서 검사하거나 전역 호환성 행렬을 만드는 것은 기본 설계가
아니다. 전체 부모 간 호환성 O(d1*d2) 검사는 비용 위험이 커 후순위다.

## 5. 복잡도 분석

### 5.1. 변수와 모델

- n: residual target을 채우는 미노 수. 빈 셀 N=4n.
- R: 전체 canonical placement catalog의 row 수.
- I: cell-row incidence 수. 각 row가 네 셀을 점유하므로 I=4R.
- d(c): 셀 c의 incidence 길이. D(G)는 노드에서 실제 유효한 부분집합.
- B: 필드 bit mask를 저장하는 머신 워드 수(고정된 compact/Extended
  표현이면 구현 범위 내 상수, 일반 크기 입력에서는 변수).
- g: 강제 3-cell 그룹 수.
- S: residual memo 등을 반영하고 실제 방문/확장하는 geometry 상태 수.
- K: 명시적으로 열거해야 할 기하 배치 조합 수.

R은 ILC realization의 원행 조건, canonical projection, 영역 크기 등에도
의존하므로 일반 ILC catalog에서 R=O(n)을 가정하지 않는다.
정상적인 정적 연결 미노와 고정 7종/19개 방향의 경우 각 셀을 포함하는
geometry row 수는 4*19 이하이므로 N개 빈 셀 안에 들어가는 기하 후보
수는 O(N)이다. 이것은 그런 catalog 부분집합에 대한 조건부 상한이며
실제 inverse-clear row/realization 수의 복잡도를 대체하지 않는다.

### 5.2. 계산 단계별 비교

| 연산 | 확인한 기존 방식 | 제안 및 조건부 상한 |
|---|---|---|
| Compact ArmPair 보조 인덱스 구축 | 최대 12R 연관 레코드 + sort/dedup, O(R log R) 비교 상한 | 완전 제거 시 해당 추가 구축 비용 없음; 직접 연결 partial 생성은 row당 상수 개 |
| Extended ArmPair 보조 인덱스 구축 | BTreeMap 삽입/정렬, O(R log R) 비교 상한; mask 비교 비용 별도 | 기존 full incidence 재사용 또는 증명된 정적 부모 직접 생성 |
| APDP 추가 인덱스 공간 | O(R) 레코드, 큰 mask 표현 시 word 비용 별도 | 별도 인덱스 제거 시 해당 공간 없음; 기존 catalog 및 scratch는 남음 |
| 부모 domain | pivot incidence의 O(d(c)) scan, mask 처리 O(B) | 정적 3-cell의 최대 8개 geometry 후보; row lookup/identity 복원 비용 별도 |
| completeness 후 재확인 | 추가 incidence/realization scan 및 index lookup | 같은 집합임을 증명했으므로 생략 가능 |
| SameTile 폐쇄 | 현재 검사 경로는 그룹/pivot 결정 | 영향 incidence만 갱신; 단조 전파의 엄밀한 비용은 추가 증명 |
| 3-cell 경합 매칭 | 이 위치에 전용 매칭 없음 | 정적 bounded-degree 그래프에서 O(g^(3/2)) 시간/O(g) 공간 |
| 실제 모든 기하 조합의 물질화 | 출력량에 의존 | 동일 결과 계약이면 출력량 하한 유지 |

정적 3-cell의 직접 부모 생성은 catalog의 해당 row lookup도 상수 시간에
가능할 때 O(1) 판정이다. 기존 incidence 자체가 상수 차수인 정적 영역에서는
부모 검사에 대한 Big O 개선이 아니라 상수항 감소일 수 있다.
Extended mask 연산, 유효 realization 검사, 상태 복원 비용을 숨기지 않는다.

전파가 없을 때 전체 geometry 탐색 비용은 개념적으로
catalog 구축 + sum(T_node(s), s in visited states) + 출력 비용이다.
새 전파를 적용하면 T_node가 증가하는 대신 S가 감소할 수 있다.
단조 폐쇄나 매칭이 임의 입력에서 지수항을 낮춘다는 증명은 현재 없다.

### 5.3. 명시적 출력의 하한 및 의존성 병목

K개의 완전한 배치 조합을 각각 n개의 row ID로 실제 생성해야 한다면,
단어 단위 출력 모델에서 최소 Omega(K*n)의 시간/출력량이 필요하다.
스트리밍에서는 동시에 유지해야 하는 메모리의 하한으로 해석하면 안 된다.
압축된 family DAG, count-only 또는 existence-only 같은 계약에는
같은 명시 출력 하한을 그대로 적용할 수 없다. 기존 family DAG와
component composition을 새 연구 성과로 중복 계산하지 않는다.

APDP가 모든 불가능한 geometry state를 제거해도 실제 존재하는 조합 K는
줄지 않는다. 또한 placement dependency 검증은 APDP의 범위 밖이다.
이번 연구의 성과로 dependency stage의 Big O 감소를 주장하지 않는다.
전체 최악 복잡도 감소는 별도 알고리즘과 엄밀한 증명이 필요하다.

## 6. 연구 및 검증 계획

아래는 구현 전에 검토할 연구 작업이며 완료된 변경 목록이 아니다.

| 단계 | 검증 대상 | 통과 기준 |
|---|---|---|
| A: 소스 동등성 감사 | completeness 성공 전후 row ID 집합 및 pivot | compact/Extended와 관련 C 소비자, proof/identity 감사; 동일 집합 및 순서 확인 |
| B: APDP 계산 축약 | 중복 검사 제거, 인덱스 생략 | canonical geometry/realization 전체 동일; admission 및 메모리 보고 정확 |
| C: 정적 직접 부모 | 모든 3-cell Arm/Elbow와 7종의 모든 방향 | 실제 catalog와 전수 동등; 경계/구멍/초기 필드/비정적 lifted row fallback |
| D: SameTile 폐쇄 | 부분 점유 row 제거와 고정점 | 실제 strict pruning witness, 독립 brute-force exact-cover와 전체 해법 대조 |
| E: 강제 row 처리와 증분 | unary 경로 축약, undo 및 공급 상태 갱신 | 동일 family/CountAll/순서; 형제 분기 및 확장 복원 안전 |
| F: 국소 매칭 | 경합 검출 및 Hall 반례 | 매칭 실패의 no-false-negative 증명; 매칭 성공을 충분조건으로 오용하지 않음 |

모든 단계는 후보의 canonical cell ownership과 piece kind, 모든 realization,
기하 family와 downstream dependency/coverage 결과를 비교한다. 전체
목표 및 공개 결과의 exactness, completeness, cancel/resource-incomplete
계약을 유지한다. proof-carrying pruning 경계는
[pruning policy](../pruning-policy.md)에 따른다. 로컬 domain 증거를
전체 clear-state에 대한 후보 삭제 권한으로 승격하지 않는다.

테스트/계측은 기존의 작은 fixture, 실제 P7P4/P7P7/P1P7P7, 여러 초기
필드, 큐와 옵션 조합, compact/Extended, 지원 kick profile을 포함한다.
비싼 full search는 정규 단위 테스트가 아니라 제한된 별도 벤치마크로
구분한다. 결과 동등성이 먼저이며 성능은 그 후에 확인한다.
기존 100% 확률 결과의 재증명은 범위 밖이다.

별도 실험에서 패리티 및 다른 pruning 설정은 동일하게 유지한다.
동일 환경에서 ABBA 순서 및 반복 측정으로 cold/warm 차이를 분리하고,
작은 입력과 큰 입력을 모두 계측한다. 기존 local-search-ab 카운터
(apdp_static_checks, apdp_static_complete, apdp_pivot_tightenings 등)를
재사용한다. 보조 계측은 다음을 분리한다.

- catalog/ILC 시간, APDP index 구축 시간/peak/retained bytes
- CellSupport 및 SameTile scan 수, APDP completeness/recheck 시간과 횟수
- 실제 추가 제거된 row/state 수, 강제 row 수, closure queue 이벤트
- 매칭 적용 횟수, 그래프 크기, 추가 prune 수
- geometry states, family nodes, K, explicit enumeration time
- downstream dependency 검증 시간과 전체 wall time
- peak RSS, scratch/undo 및 워커별 증가량, serial/tail 시간

논리 프로세서 기반 워커 정책과 기존 관리 워커 분리를 유지한다.
공유 immutable catalog를 워커별 복제하지 않고, 부가 메모리 증가와
자원 실패 시 incomplete 또는 exact fallback 계약을 검증한다.
비싼 벤치마크에는 저장소 AGENTS.md의 런타임 관리 정책을 적용한다.

## 7. 우선순위 및 결론

우선순위는 (1) 기존 APDP 재검사의 동등성 감사 및 제거, (2) 별도 인덱스
의존성 축소와 정적 부모의 직접 완성, (3) SameTile 폐쇄와 강제 row의
일괄/증분 전파, (4) 실제 경합 영역에 대한 국소 이분 매칭이다.
각 단계에서 별도의 full-row 호환성 표나 패리티를 무조건 추가하지 않는다.

핵심은 ArmPair의 존재 자체가 아니라 **반드시 같은 미노에 속하는 부분도형의
완전한 부모 도메인**이다. 이 사실에서 중복 APDP 검사를 제거하는 정리와
정적 연결 도형의 직접 완성 정리를 얻었다. SameTile 폐쇄와 매칭은
추가 가지치기의 설계 후보다. 전역 최악 Big O 감소나 실제 가속률은
아직 증명/측정되지 않았다.

이번 커밋은 연구 문서만 추가한다. 제품 코드, 설정, 기존 규약,
의존성 알고리즘, 배포 및 CI 구성은 변경하지 않는다. main 반영이나
release 승인을 의미하지 않는다.

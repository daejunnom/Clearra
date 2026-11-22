# 의존성 순환, 물리 실행 언어, 원본 큐 교집합의 정확성

## 범위와 완료 기준

2026-11-23 사용자 정정에 따라 Full 6L의 완료 기준은 전체 후보 영역 소진,
정확한 정규화 해답 집계, 원본 큐 확률 계산이다. 원본 큐 coverage 100%를 먼저
얻었다는 사실은 전체 해답 열거의 완료 증거가 아니다.

검토 대상 입력은 빈 10×6, SRS+, 15 locks, 유한한 `P1P7P7`, 초기 빈 Hold,
FullQueueOracle이다. 원본 큐는 `7 × 5,040 × 5,040 = 177,811,200`개이며
첫 atom의 한 draw와 뒤의 두 7-bag를 실제 순서 그대로 해석한다. EOF에서는
이미 보유한 held piece만 마지막으로 사용할 수 있고, 보이지 않는 다음 bag의
piece를 새로 공급하지 않는다.

이 문서는 현재 구현의 증명 범위와 회귀를 소스에서 검토한 기록이다. 일반적인
정적 의존성 그래프가 비순환이라는 사실만으로 Full 6L의 물리 실행 가능성이나
원본 큐 교집합의 비어 있지 않음을 증명하지 않는다. 11미노 see-7의 causal policy
문제는 이 검토 범위에 포함하지 않는다.

## 1. 구분해야 하는 세 관계

고정된 기하 후보 `C`에 대해 다음을 구분한다.

| 관계 | 상태와 의미 | 성공 또는 거절에 사용할 수 있는 근거 |
| --- | --- | --- |
| 정적 필수 선행 관계 | 모든 물리 실행이 지켜야 하는 concrete operation 간 순서 | 필수 선행 관계의 순환은 불가능성의 충분조건이다. 비순환만으로 물리 실행을 증명하지 않는다. |
| 물리 실행 관계 `G(C)` | 실행한 operation subset, 정확한 원본 줄 삭제 frame, 실제 SRS lock transition | root에서 accepting terminal로 가는 경로가 있어야 한다. 각 edge가 operation 하나를 추가하므로 이 그래프 자체는 항상 DAG다. |
| 원본 source/Hold 교집합 | 물리 관계의 kind-word 언어와 각 원본 큐의 실제 Hold/EOF 실행 언어 | 같은 원본 ordinal에서 둘을 모두 만족하는 word가 있을 때만 그 큐가 후보를 성공시킨다. |

`BuildOrderGraph::from_topological_parts`는 edge의 도착 index와 depth 증가를
검증한다. 이 일반 생성자 자체는 물리 reachability를 증명하지 않는다. 물리
권한은 `BuildOrderGraph::build`의 정확한 transition 생성이나 별도로 검증된
생산자에게 있다. 따라서 테스트용으로 만든 live chain을 물리적으로 실행 가능한
실제 물리 배치라고 해석해서는 안 된다.

관련 소스:

- [buildup.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs)
- [placement_poset.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/placement_poset.rs)
- [realization_feasibility.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/realization_feasibility.rs)

## 2. 현재 코드에서 성립하는 동치

`L_phys(C)`를 정확한 물리 실행 관계의 accepting path에서 읽은 kind-word들의
집합으로 둔다. `H(q)`는 고정된 원본 큐 `q`, 초기 Hold, 실제 finite EOF 계약으로
만들 수 있는 출력 kind-word들의 집합이다. 요청된 FullQueueOracle의 후보 판정은
다음 관계다.

```text
candidate C succeeds on original queue q
    iff L_phys(C) ∩ H(q) is nonempty.

candidate C is buildable for the original universe U
    iff some q in U has a nonempty intersection above.
```

첫 번째 동치는 정확한 물리 관계를 사용하고, 실제 source와 Hold/EOF 계약을
모두 보존한다는 조건을 가진다. geometry-only transition, 정적 predecessor
상위 집합, 다른 source의 동일 cardinality는 이 조건을 대신하지 못한다.

`CountUnique`의 전체 결과는 이 판정을 통과한 모든 정규화 후보 identity의
집합이다. 한 후보가 하나의 큐에서 성공했다는 양성 증거는 그 후보의 membership을
증명하지만, 다른 후보들의 판정이나 전체 geometry domain 소진을 증명하지 않는다.
`CountAll`의 operation/실행 경로 multiplicity는 별도 그래프와 weighted product의
권한이며, Boolean kind-language quotient로 바꾸지 않는다.

### CertifiedPlacementPoset에만 적용되는 추가 동치

`placement_poset.rs`는 먼저 모든 물리 실행에 필요한 선행 관계를 도출한 뒤,
그 poset의 모든 available ideal transition이 grounded하고 정확한 ILC realization과
이동 증거를 갖는지 별도로 증명한다. 두 방향을 모두 증명해
`Some(CertifiedPlacementPoset)`을 반환한 경우에만 valid operation orders가 그
poset의 linear extensions와 정확히 같다.

이 인증에서도 source/Hold 교집합은 별도로 필요하다. `compile`의 `None`은
지원하지 못했거나 증명을 완성하지 못했다는 fallback이며, 빈 물리 언어라는
결론이 아니다. 반면 `CertifiedConditionalLanguage.root == None`은 해당 생산자가
완결된 관계에서 증명한 빈 언어다. 두 종류의 `None`을 같은 뜻으로 사용하면 안 된다.

## 3. ILC 의존성 closure의 정확한 단방향 계약

`dependency_relaxation_stalls`는 단순한 binary edge cycle 검사가 아니다.

1. operation은 concrete ILC realization 중 하나라도 필요한 줄 삭제 조건을
   만족하면 closure에 들어갈 수 있다. realization 대안은 OR다.
2. 한 원본 row는 그 row에 기여하는 모든 operation이 closure에 들어간 뒤에만
   삭제되었다고 간주한다. row contributor 조건은 AND다.
3. 더 넣을 operation이 없는 least fixed point가 전체 operation에 도달하지
   못하면 relaxed relation도 완료될 수 없다.

이 closure는 collision, grounding, 실제 이동을 완화한다. 모든 실제 실행이
closure 안에 포함되므로 **stall이면 실제 실행과 가능한 원본 큐도 없다**.
closure가 전체에 도달했을 때는 정확한 물리 및 source 검사를 계속해야 한다.
한 realization의 순환을 모든 realization에 대한 불가능성으로 바꾸거나 OR 대안을
필수 AND edge로 합치면 이 증명은 유지되지 않는다.

기존 회귀 `relaxed_ilc_rejects_the_two_row_dependency_cycle`은 서로의 원본 row
삭제를 요구하는 두 operation의 stall을 검사한다.
`relaxed_ilc_accepts_an_acyclic_dependency_chain`은 시작 가능한 operation이 있는
chain에서 stall을 반환하지 않는지 검사한다. 후자는 물리 실행 성공 테스트가 아니다.
현재 optional complete realization prepass는 제품 기본값에서 비활성이고,
비활성 반환은 `Unknown`이다. plain product의 정확한 BuildUp이 최종 권한을 가진다.

### Partial prefix sufficient condition: 모든 completion에 대한 음성 증명

여기서 prefix는 family 열거기가 이미 선택한 **기하 row들의 집합**이다. 선택
순서를 물리적인 lock 순서라고 가정하지 않으며, 선택한 operation들이 실제로
먼저 실행되었다고 가정하지도 않는다. 아래 조건은 이 prefix를 포함하는 모든
완성 후보에 대한 충분한 음성 증명이다. 구현과 실행 회귀의 완료 상태는 별도로
기록하며, 이 증명을 곧바로 Full 6L 전체 열거 완료라고 해석하지 않는다.

고정된 catalog에서 최종 target의 완성 행 집합을 `F`, 이미 선택한 concrete
operation 집합을 `S`, closure에서 실행 가능하다고 완화한 부분집합을 `P ⊆ S`로
둔다. 각 원본 행 `r`의 selected contributor 집합을 `C_S(r)`로 쓰면 삭제 가능
행 집합은 다음과 같다.

```text
F      = full original rows in (catalog.initial_board | catalog.required_cells)
C_S(r) = {s in S : original target cells of s occupy row r}
D_S(P) = {r in F : C_S(r) is a subset of P}
```

아직 선택하지 않은 미래 operation은 모두 이미 완료되었다고 완화한다. 따라서
`C_S(r)`가 비어 있는 완성 행은 처음부터 `D_S(∅)`에 들어간다. 이것이 empty
AND의 참값이다. 반면 선택된 contributor가 있는 행은 그 contributor 전부가
closure에 들어간 뒤에만 available하다. 초기 고정 board의 셀은 operation으로
다시 세지 않는다.

각 selected operation `s`는 catalog에 보존된 모든 concrete realization 중
하나라도 양의 required-deleted-rows 조건을 만족하면 ready다.

```text
ready(s, P)
    iff some a in catalog.realizations(s)
        has required_deleted_rows(a) subset of D_S(P).

P_0     = empty set
P_(n+1) = P_n union {s in S : ready(s, P_n)}
```

`D_S`와 required-rows predicate가 모두 단조이므로 이 반복은 유한한 least fixed
point `P*`에 도달한다. **`P* != S`이면 이 기하 prefix를 포함하는 어떤 완성
후보도 물리적으로 실행될 수 없다.** realization 대안은 끝까지 OR이며, 각 row의
concrete contributor 조건만 AND다.

#### 실제 실행이 closure에 포함되는 이유

`S`를 포함하는 어떤 완성 후보 `C`가 실제 accepting lock 순서를 가진다고
가정한다. 그 순서에서 selected operation만 골라 순서대로 본다. 다음 selected
operation `s`를 실제로 놓기 직전까지 실행된 selected operation들이 closure에
들어 있다고 귀납 가정한다.

실제로 삭제된 원본 행은 그 행의 모든 contributor가 먼저 놓인 행이다. 따라서
그 행의 selected contributor도 모두 앞서 놓였으며, 귀납 가정에 따라 closure에
있다. 즉, 그 시점의 실제 삭제 프레임 `D_actual`은 `D_S(P*)`의 부분집합이다.
실제로 사용한 realization `a`는 `required_deleted_rows(a) ⊆ D_actual`을
만족하므로 더 넓은 `D_S(P*)`에서도 ready다. 고정점 정의상 `s` 역시 `P*`에
포함되어야 한다.

이를 모든 selected operation에 적용하면 `S ⊆ P*`다. 따라서 `P* != S`라는
stall은 실제 accepting 순서가 존재한다는 가정과 모순된다. 이 논리는 미래
operation의 배치를 실현해 주거나, 선택 순서를 강제하거나, 특정 source 큐를
가정하지 않는다. 물리적 completion 자체가 없으므로 그 뒤의 모든 source/Hold
교집합도 비어 있다.

반대 방향은 성립하지 않는다. `P* == S`는 이 완화가 거절하지 못했다는
`Unknown`이며, grounding, 충돌, exact original-frame transition, ordered kicks,
물리 kind language 및 실제 finite source/Hold 교집합 검사를 계속해야 한다.

#### Catalog 권한과 구현 경계

[catalog.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs)의
`instantiate_realization_raw`는 실제 pose를 생성하기 전에 required-deleted-rows의
포함 관계를 검사한다. `compile_skeleton_temporal_metadata`는 같은 skeleton의
모든 realization에 대해 이 양의 조건을 OR한 domain을 저장하고,
`realization_requirement_is_satisfied`는 그 domain을 조회한다. Full 6L에서는
64개 삭제 프레임이 한 `u64`에 들어간다. metadata가 없으면 같은 전체 realization
목록을 순회하는 fallback이 같은 OR 계약을 가진다.

다음 경계는 이 충분조건의 일부다.

- 기존 full-candidate `dependency_relaxation_stalls`는 `contributors != 0`인
  row만 삭제 가능하다고 본다. selected contributors만 넘기는 직접 재사용은
  미래 operation만 기여하는 행을 잘못 막는다. partial 경로는 empty AND를
  별도로 처리해야 하며 기존 full-candidate 의미를 몰래 바꾸지 않는다.
- `F`는 전체 target에서 계산한다. partial `CandidateProjection`의
  `completed_target_rows`를 사용하면 미래에 완성될 행을 빠뜨려 정답을 거절할 수
  있다. 선택되지 않은 operation을 실제로 배치했다고 가정한 board를 만들 필요는 없다.
- 판정에 쓰는 predicate는 모든 realization의 양의 required-rows OR다.
  `exact_clear_domain_mask` 또는 concrete instantiation의 occupied-row 금지 조건은
  전역적으로 단조인 predicate가 아니므로, 별도의 제한된-domain 증명 없이 이를
  closure predicate로 대체하지 않는다. pose 하나의 실패도 전체 operation 실패가 아니다.
- 이 판정은 concrete selected prefix에 종속된다. prefix를 잊는 residual-family
  memo에 이 결과를 일반적인 suffix 불가능성으로 저장할 수 없다. Append에서 얻은
  실제 prefix와 Union/Product continuation의 보존된 prefix가 판정의 입력이어야 한다.
- 지원하지 않는 범위, 잘못된 metadata, 취소 또는 자원 실패는 stall 증명과 다르다.
  이 경우 실패를 음성 해답으로 바꾸지 않는다. 정확한 판정에 필요한 입력이 없으면
  해당 최적화의 결과는 `Unknown`이다.
- subtree를 건너뛰면 기존 raw packing 순회 count/digest와 실제 emitted candidate
  영역이 달라진다. 원시 전체 영역 진단을 유지했다고 주장하지 않으며, 그 진단과
  정확한 최종 solution 집계의 authority를 구분한다. 생존 후보의 기존 순서를
  유지하고, 제외한 subtree는 위 충분조건으로 모두 음성임을 증명해야 한다.
- 많은 subtree를 한 호출에서 건너뛰면 `next_candidate`가 오래 반환하지 않을 수
  있다. 작은 closure의 유한성만으로 전체 호출의 취소 응답성을 주장하지 않는다.
  traversal work quantum이나 주기적인 취소 확인이 필요하고, 중간 yield를 완료로
  해석하지 않는다.

이미 closure가 완주한 prefix에 `requirement_is_satisfied(row, 0)`인 operation을
추가한 경우에는 새 operation을 먼저 넣고 기존 closure를 재현할 수 있다. 이
조건에서 재검사를 생략해도 음성 판정은 변하지 않는다. 일반적인 concrete-op
검사의 요구 조회 상한은 selected 수 `k`에 대해 `k(k+1)/2`회이며, 15개 operation과
6행에서는 각각 최대 120회와 `6k = 90`회의 행 AND 검사다. 추가 persistent cache는
이 증명에 필요하지 않다.

#### 이 충분조건에 필요한 회귀

future-only row의 empty AND, realization OR 대안, selected contributor AND를
각각 고정한 작은 모델이 필요하다. 이어 작은 실제 PC의 모든 완성 후보와 모든
selected subset에 대해 `partial stall ⇒ full relaxed stall`을 비교하고, 독립 exact
Build를 통과한 후보의 모든 selected subset에서는 stall이 없어야 한다. serial과
parallel의 전체 normalized 결과 집합 및 원본 coverage parity도 확인해야 한다.

`geometric_edge_reuse_product_default_matches_full6l_mixed_finite_graph`의 실제
10×6, 15-operation fixture는 I 3개와 나머지 각 2개로 이루어지며, 하단 원본 4행
동시 삭제 후 상단 2행 삭제의 물리 증인을 가진다. `fb9ab33a`에서 이 fixture의
정확한 graph, 32,768개 subset의 scalar projection, 원본 P1P7P7 source 교집합
parity가 실행 검증되었다. 이후 `0fbec64f`에서는 같은 fixture의 **32,768개 모든
selected subset과 각 역순 재인덱싱에 대한 partial no-stall**도 실제로 실행해
통과했다. 이는 기존 scalar projection 검증과 별도로 수행한 closure 양성 회귀다.

추가 물리 회귀는
[geometry_prefix_witness_tests.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_prefix_witness_tests.rs)에 있다.
아래 세 테스트는 `0fbec64f`의 `physical_prefix` 필터 실행에서 모두 통과했다.

| 테스트 | 확인하는 경계 |
| --- | --- |
| `physical_prefix_preserves_all_32768_subsets_of_full6l_mixed_witness` | 검증된 mixed15 fixture의 scalar drop/원본행 clear를 다시 확인하고, 모든 selected subset과 그 역순 재인덱싱에서 no-stall을 검사한다. |
| `physical_prefix_keeps_future_only_gap_rows_of_a_live_ilc_witness` | 4×4 `initial=0x3030`에서 O `0x00cc → 0xcc00 → 0x0303`의 물리 drop을 검사한다. 마지막 O는 원본 rows0,2를 차지하고 row1 삭제가 필요하다. 이 O만 selected인 경우의 future-only row와 전체 8개 selected subsets를 보존한다. |
| `physical_prefix_rejects_a_selected_cycle_in_the_real_ilc_catalog` | 실제 O skeleton `0x0303`과 `0xc0c0`의 모든 realization 요구를 확인한다. 각각 단독 prefix는 통과하지만 둘이 서로의 row 삭제를 요구하는 prefix는 stall이다. 완성된 4-O 타일링에서 첫 concrete instantiation이 없다는 사실도 확인한다. |

### 24개 내부 행 순서와 future-family mask: 증명된 제안, 미구현·미측정

이 절은 후속 구조 개선안의 수학적 검토다. **아래 row-order capability,
24-bit skeleton mask, FamilyDAG의 `Arc<RowOrderTable>` 보조 표는 아직 구현하거나
측정하지 않았다.** 현재 concrete-operation prefix closure의 실행 결과와
혼동하지 않는다. 같은 물리 음성 증명을 더 이른 suffix 경계에서 사용하는
제안이며, 새로운 물리적 양성 판정이나 source 완료 기준을 제안하지 않는다.

#### 적용 capability와 skeleton mask

빈 10×6 Full 6L의 전체 target 6행이 완성되고, 해당 immutable catalog의
**모든 realization**에서 `required_deleted_rows ⊆ {1,2,3,4}`임을 확인한 경우를
대상으로 한다. 행 번호는 0부터 센 원본 row다. row 0과 row 5는 어떤 operation도
삭제를 요구하지 않는 sink이므로, 낙관적 모델에서는 마지막에 처리할 수 있다.
이 capability는 실제 catalog 내용으로 확인해야 하며, height나 skeleton 개수만
보고 부여하지 않는다. 해당 조건이 성립하지 않으면 이 24-order 동치를 사용하지
않고 기존 concrete closure로 fallback해야 한다.

현재 standard tetromino catalog의 `enumerate_row_projections`는 occupied target
rows 사이의 gap만 required-deleted rows로 만든다. 6행 내부의 그런 gap은 row
1..4에만 있다. 그래도 제안된 capability는 모든 보존된 realization을 확인하여
소스 계약을 고정하는 방식이다. realization 하나를 제외한 표로 capability를
얻거나, outside-row requirement를 무시한 채 같은 동치를 주장하지 않는다.

`I={1,2,3,4}`의 24개 순열을 `Π`라고 둔다. skeleton `s`가 점유하는 원본 행
집합을 `O_s`, realization `a`의 양의 required rows를 `R_a`라고 하면 허용 순열은
다음과 같다.

```text
A_s = {π in Π : some a in realizations(s) satisfies
                  π(r) < π(o) for every r in R_a and every o in (O_s ∩ I)}.

M(S) = intersection of A_s over selected operations s in S.
M(empty selected set) = all 24 orders.
```

하나의 realization에서는 **모든 required 행이 모든 occupied 내부 행보다 먼저**
와야 한다. 서로 다른 realization의 대안은 OR다. `R_a`가 비어 있거나 occupied
내부 행이 없으면 이 비교는 공집합에 대한 전칭 조건이므로 참이다. 특히 내부
행을 전혀 점유하지 않는 operation은 required 내부 행들을 모두 처리한 뒤 놓으면
된다. 이를 임의 rank 기본값으로 잘못 표현해 제외하지 않는다.

#### 동치: 공통 순열 존재와 기존 낙관적 closure의 완주

위 capability 아래에서 다음 동치가 성립한다.

```text
M(S) is nonempty
    iff the optimistic selected-operation closure reaches every operation in S.
```

**closure에서 공통 순열로.** closure가 완료되면 operation들이 ready가 된
순서대로 하나씩 넣는 순차 실행을 택할 수 있다. 각 operation에는 그 시점에
요구를 만족시킨 realization 하나를 고정한다. required 행은 operation 이전에
이미 삭제 가능했고, occupied 행은 그 operation 자신이 contributor이므로
그 operation 이후에야 삭제 가능하다. 따라서 required 행의 삭제 시각은 모든
occupied 행의 삭제 시각보다 엄격히 빠르다. 내부 행의 삭제 시각을 정렬하면
모든 operation의 조건을 동시에 만족하는 순열을 얻는다. 같은 시각에 삭제되는
행들 사이에는 이런 엄격한 required→occupied 조건이 없으므로 tie를 임의로
정렬해도 된다. joint clear는 반례가 아니다.

**공통 순열에서 closure로.** `π ∈ M(S)`를 고르고 각 operation마다 이 순열에
맞는 realization을 하나씩 택한다. 내부 행을 `π` 순서로 처리하면서, 각 행을
삭제하기 직전에 그 행이 자신의 첫 occupied 내부 행인 operation들을 넣는다.
이 operation들의 모든 required 행은 앞선 순서에 있으므로 이미 처리됐다.
그 행에 기여하는 다른 selected operation들은 자신의 더 이른 첫 occupied 행
단계에서 이미 들어왔다. 따라서 해당 행의 contributor AND도 만족된다.
내부 행을 전혀 점유하지 않는 operation은 내부 4행 이후에 넣으면 된다.
row 0과 row 5는 어떤 요구의 전제도 아니므로 나중에 처리해도 진행을 막지 않는다.
이 순차 진행은 낙관적 관계 안에 있으므로 least fixed point는 `S` 전체를 포함한다.

이 순열은 양의 required→occupied precedence를 함께 만족하는 **증명용 total
order**다. 실제 게임에서의 줄 삭제 trace나 동시 clear의 시간 정보를 직접
인코딩하는 것이 아니다. 빈 contributor 행이 실제 closure에서 더 일찍 available해져도
양의 요구만 있으므로 진행을 막지 않는다. 이 단조성이 두 번째 방향의 근거다.

#### FamilyDAG에서의 정확한 존재성 합성

`F(N)`을 기존 FamilyDAG node `N`이 열거하는 row-path family라고 두고, node의
가능 순열을 다음처럼 정의한다.

```text
B_N = union over c in F(N) of (intersection of A_s over rows s in c).
```

한 bit `π`의 의미는 **그 순열과 양립하는 path가 node family 안에 하나 이상
있다**는 것이다. 서로 다른 bits의 witness path는 달라도 된다. 이 정의에는
다음 recurrence가 정확히 성립한다.

| Node | 24-bit 가능 순열 mask |
| --- | --- |
| `FAMILY_INVALID` | `0` |
| `FAMILY_EMPTY` | `all24` |
| `Append(s, child)` | `A_s & B_child` |
| `Union(left, right)` | `B_left | B_right` |
| `Product(left, right)` | `B_left & B_right` |

Append는 고정 row 하나의 조건을 추가한다. Union은 두 family 중 하나를 택한다.
Product에서는 고정한 **같은 순열**에 대해 왼쪽과 오른쪽에서 각각 compatible
path가 하나씩 존재하면 두 path를 결합할 수 있다. 즉, AND가 OR 위에 분배되는
Boolean bitset 대수로 정확한 existential recurrence를 얻는다.

이 Product 증명은 기존 family의 Cartesian 의미에 의존한다.
[geometry_family.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_family.rs)의
`path_count_for_reference`도 Product를 두 child count의 곱으로 계산하고,
[geometry.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs)의
continuation traversal은 각 왼쪽 path에 오른쪽 family 전체를 결합한다. 공유된
DAG node는 재사용되는 family 값이며, 여러 참조가 하나의 숨은 Union 선택을
공유하는 변수가 아니다. 외부의 상관된 선택 제약이 추가된다면 그 제약을 DAG에
명시적으로 표현해야 하며 위의 Cartesian 증명을 그대로 확장할 수 없다.

예를 들어 `A`가 `row1 < row2`만 허용하고 `B`가 `row2 < row1`만 허용하면 각
child mask는 nonzero다. 그러나 `Product(A,B)`는 공통 순열이 없으므로 0이다.
반대로 `Union(A,B)`는 두 대안 모두를 유지해야 한다. 각 child의 nonzero 여부만
따로 검사하거나, 서로 다른 순열을 선택하게 하는 계산은 Product 존재성을
증명하지 못한다.

현재 traversal task의 selected-prefix mask가 `Pmask`, 현재 node가 `N`, 남은
Product continuation들이 `K_1 .. K_m`이면 다음 교집합을 사용한다.

```text
feasible_orders = Pmask & B_N & B_K1 & ... & B_Km.
```

0이면 현재 task의 **어떤 future family path 조합도** 기존 낙관적 closure를
완주할 수 없으므로 subtree 전체를 음성으로 제외할 수 있다. nonzero는 해당
낙관적 제약에 대한 존재성일 뿐이며 여전히 물리·source 성공에는 `Unknown`이다.
후속 exact graph, SRS movement, finite Hold/EOF와 원본 큐 product는 유지한다.
표는 후보 수나 identity별 multiplicity를 보관하지 않는다. 같은 순열이 여러 후보를
허용하고 같은 후보가 여러 순열을 허용할 수 있으므로 mask의 popcount나 node별
bit 수를 후보 수·해답 수로 해석하지 않는다. 기존 raw family count 및 정확한
생존 후보의 검증·집계는 별도 권한이다.

이 방식은 prefix에 의존하는 단일 pass/fail 값을 residual memo에 저장하는 것과
다르다. node 표는 그 family가 허용하는 **모든** 24-order 가능성을 보존하고,
조회 시 현재 prefix 및 모든 continuation의 조건과 교차한다. 따라서 동일 suffix
DAG가 서로 다른 prefix에서 재사용되어도 조건이 섞이지 않는다.

#### 제안된 저장 방식과 필요한 차등 검증

승인된 구현 방향은 `RowOrderTable { values: Vec<u32>, ... }`를 별도의 immutable
`Arc<RowOrderTable>`로 공유하는 것이다. `Vec`의 기존 buffer를 그대로 소유하고
owner로 옮기므로 큰 payload를 `Arc<[u32]>`로 변환하면서 복사하거나 두 buffer를
동시에 보유하는 구간을 만들지 않는다. 약 5,200만 node이면 payload만
`4N ≈ 208,000,000 bytes`(약 198 MiB)이며, 실제 Vec capacity, 구조·allocator
overhead와 생성 중 peak는 별도 계상한다. 이는 계산상 비용 추정이고
실제 메모리·시간 측정 결과가 아니다. 표는 해당 catalog와 frozen family의 node
ID namespace에 결속되어야 하며 같은 node 수라는 이유로 다른 owner에 재사용할
수 없다. partial table이나 allocation failure를 0 mask라는 음성 증명으로
바꾸지 않고 기존 prefix closure로 fallback해야 한다.

구현 전후에는 최소한 다음 차등 검증이 필요하다.

- 실제 catalog의 모든 보존 realization에 대한 capability 확인과, capability가
  없는 경우의 fallback. empty required/empty occupied-internal 및 boundary-only
  operation도 포함한다.
- 실제 ILC 양성·순환 음성 fixture와 작은 전체 geometry 영역의 selected subsets에서
  `24-mask nonzero == 기존 concrete optimistic closure pass` 비교. OR 대안을 가진
  작은 모델도 별도로 유지한다.
- 작은 Append/Union/Product DAG를 명시적으로 전수 열거해 node별 24 bits를 각각
  비교한다. nonzero child들의 Product가 0인 경우, 그 Union은 nonzero인 경우,
  shared node, invalid/empty node, 여러 Product continuations를 포함한다.
- 기존 prefix-only traversal과 future-mask traversal의 전체 emitted 순서 및
  최종 normalized identity/원본 coverage 동치. 더 일찍 거절된 subtree의 raw
  count 분리는 유지하고, 취소/yield/parallel seed의 상태 보존도 재검사한다.

## 4. 저장소에 있는 구체적인 반례

다음 테스트들은 [structural_graph_probability_tests.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/structural_graph_probability_tests.rs)에 있다.

### Grounding OR는 하나의 predecessor conjunction으로 표현되지 않는다

`disjunctive_grounding_is_unknown_and_never_an_infeasibility_prune`는 서로 다른
두 O placement가 위의 I를 각각 지지할 수 있는 후보를 검사한다. 물리 kind-word에는
`OOI`, `OIO`가 있고 `IOO`는 없다. concrete operation 기준으로 I는 두 O 중
하나를 필요로 하므로 단순 AND 선행 관계로 전체 관계를 표현할 수 없다.
poset 생산자는 이 경우 `None`으로 fallback한다.

같은 테스트의 roof fixture는 수직 hard-drop corridor가 막혀 있어도 실제 kick
oracle이 `OI`와 `IO`를 모두 허용하는 경우를 포함한다. hard-drop 인증 실패는
실제 이동 불가능성의 증거가 아니다.

### 줄 삭제 뒤의 이동 조건은 추가 선행 관계를 만들 수 있다

`clear_can_close_a_translation_entry_and_requires_a_movement_predecessor`의
10×3 fixture는 다음과 같다.

```text
initial = 0x3f1fc3f7
T cells = 0x201c00
L cells = 0xc02008
```

이 테스트는 SRS+, SRS90, SRS-X, Jstris180, NoKick의 실제 oracle에서 물리 언어가
정확히 `{TL}`임을 검사한다. L을 먼저 놓아 원본 row를 지우면 T의 진입로가 닫힌다.
grounding/row contributor 사실만으로는 필요한 `T < L` 이동 조건을 얻지 못한다.

이 검사 결과와 Hold 규칙에서 바로 따라오는 별도 예는 source가 고정 큐 `LT` 하나이고
Hold가 금지된 경우다. 물리 DAG에는 accepting path `TL`이 있지만 source/Hold
교집합은 비어 있다. 이것은 일반적인 비동치의 예이며, 빈 Full 6L/Hold 허용 입력에
한정한 기하 반례라고 주장하지 않는다.

### 같은 compact board도 같은 ILC 상태가 아니다

`identical_compact_board_changes_pending_i_lock_across_original_clear_frames`는
SRS90에서 compact board `0x3`가 같아도 원본 삭제 frame이 `0`과 `0b011`일 때
같은 pending I operation의 합법성이 달라지는 것을 검사한다. 원본 row 2의 I가
나중에는 floor의 pose로 투영된다. 따라서 compact occupancy 또는 pairwise
dependency만으로 정확한 상태를 합칠 수 없다.

`one_i_operation_keeps_opposite_ordered_kick_answers_for_its_two_realizations`는
같은 I lock mask의 두 rotation realization이 각각 불가능/가능한 사례를 검사한다.
한 representative realization의 실패를 operation 전체의 실패로 넓힐 수 없다.

## 5. 빈 물리 언어와 빈 source 교집합의 전달

현재 [buildup.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs)의
ordinary verifier는 `graph.nodes[graph.root].live`일 때만 coverage product를
실행한다. dead root이면 coverage가 없고 최종 `buildable=false`다.

[piece_order_language.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/piece_order_language.rs)의
`canonicalize_and_mark_live`는 dead root에 `Ok(None)`을 반환한다.
`fused_language_dead_root_cannot_reuse_previous_graph_classes`는 동일 owner에서
live → dead → live를 실행해 이전 graph class가 dead root를 살려내지 못하게 한다.
`conditional_language_exact_empty_and_dead_branches_are_not_unknown`은 완결된
조건부 관계의 빈 언어와 일부 dead branch의 의미를 구분한다.

live kind language가 source와 만나지 못하면
[standard_bag_coverage.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage.rs)의
정확한 product가 zero original coverage와 no witness를 반환해야 한다.
`certified_language_result`는 `covers_any_pattern`이 false인 경우
`buildable=false`, `symbolic_coverage_root=None`을 유지한다.

`covers_any_pattern`의 공통 계약은 실제 materialized original coverage count가
0이 아닌지로 판단한다. truncated-prefix source에서는 symbolic full source의
non-REJECT root만으로 retained ordinal의 존재를 주장할 수 없다.
`truncated_standard_bag_prefix_cannot_make_a_candidate_buildable`이 이 consumer
경계를 검사한다. complete factorized source는 전체 원본 ordinal을 보존하므로
이 차이가 나타나지 않지만 같은 공통 계약을 사용한다.

## 6. Factorized source의 pointwise 계약과 추가 회귀

[factorized_source.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage/factorized_source.rs)는
실제 complete uniform backing에서만 layout을 얻는다. atom의 choice 순서,
draw 수, 경계 reset, suffix count, full/visible 길이를 보존한다. 입력 문자열이나
같은 pattern count는 source authority가 아니다.

`StandardBagCoverage::solve`는 current, swap-held, store-current의 실제 공급
transition과 kind edge를 함께 소비한다. finite factorized EOF에서는 held source
piece의 마지막 사용만 허용하고 Standard7Bag의 unseen complement 분기를 차단한다.

관련 테스트는 [factorized_tests.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage/factorized_tests.rs)에 있다.

| 테스트 | 확인하는 범위 |
| --- | --- |
| `factorized_coverage_matches_every_original_queue_and_canonical_hold_branch` | `P1P4`, `P2P3`, `P1P6`, `[LOI]2P5`의 모든 원본 ordinal을 canonical finite supply automaton과 비교. Hold 금지/빈 Hold/held T를 포함한다. |
| `factorized_partial_atom_eof_cannot_place_its_unseen_complement` | finite partial atom EOF에서 미공개 complement를 사용할 수 없고 실제 held piece만 사용할 수 있다. |
| `factorized_p1p7p7_rank_boundaries_keep_exact_witnesses_without_expanding_source` | Full P1P7P7의 대표 원본 rank 경계 3곳에서 정확한 witness를 유지한다. 전체 177M 큐 전수 회귀라고 해석하지 않는다. |
| `factorized_live_language_empty_intersection_matches_all_108_original_queues` | 아래 108개 큐를 실제 factorized `P1P3P3`의 동일 원본 ordinal에 연결하고, negative/positive/negative를 같은 owner에서 실행한다. 전체 bitmap, count, witness와 나머지 ordinal의 음성 결과를 비교한다. |
| `factorized_p1p7p7_live_matching_inventory_language_can_have_no_source_queue` | matching inventory인 15-kind live chain도 Full P1P7P7/Hold 교집합이 비어 있을 수 있음을 symbolic count와 witness로 검사한다. |

### 108개 원본 큐의 명시적인 empty intersection

source `[IOT]1[IOT]3[IOT]3`은 `3 × 6 × 6 = 108`개 원본 큐를 갖는다.
live acyclic kind chain `IIIOOTT`는 그중 첫 draw가 I인 큐와 같은 inventory다.
그러나 세 번째 I를 만나기 전에 첫 3-piece bag의 non-I 두 개를 통과해야 한다.
초기 빈 Hold는 한 개만 미룰 수 있으므로 처음 세 locks를 `III`로 만들 수 없다.

새 회귀는 기존 `finite_oracle`의 canonical `SupplyExecutionAutomaton::sequence`
transition으로 108개를 모두 검사한다. 실제 source의 ordinal 0을 kind chain으로
만든 positive control도 비교한 다음 동일 negative root를 다시 검사한다. 하나의
language owner에 두 root를 추가하여 numeric root ID의 namespace를 보존한다.

제품 parser는 4,096개 이하의 source를 explicit storage로 보관한다. 따라서 이
108개 source 자체가 factorized factory를 지원한다고 가정하지 않는다. 테스트는
실제 factorized `P1P3P3`의 308,700개 원본 중 동일한 길이 7 큐의 ordinal을 찾고,
각 mapping을 실제 `sequence_at` 결과와 대조한다. 두 chain은 I/O/T만 사용하고
모든 draw를 소비하므로 나머지 원본 큐는 성공할 수 없다. 108개 canonical oracle로
만든 expected bitmap을 product의 전체 bitmap과 비교하여 이 나머지 영역도
음성임을 확인한다. Hold나 finite EOF 규칙, 원래 108개 큐의 내용은 바꾸지 않는다.

### Full P1P7P7의 matching-inventory 음성 사례

`IIIOOTTSSZZJJLL`은 15개이며 I가 3개, 나머지 종류가 각각 2개다. 이는 P1에서
I를 선택한 source inventory와 일치한다. 그러나 세 번째 I는 두 번째 7-bag까지
도달해야 나오므로 같은 `III` prefix가 불가능하다.

새 회귀는 이 nonempty chain의 product가 `REJECT`, count 0, witness None인지
검사한다. ordinal 0의 실제 source word를 positive control로 사용하고 negative를
다시 방문한다. 177,811,200-bit bitmap은 materialize하지 않는다.

두 chain 회귀는 **source/Hold 교집합**의 증거다. 해당 chain만을 갖는 실제 Full 6L
기하 후보를 구성하거나, 모든 Full 6L 기하 후보의 성질을 증명한 것은 아니다.

## 7. 실행 검증 상태

`d9225aa1`의 통합 factorized 테스트는 **8/8 통과**했다. 첫 실행에서는 작은
108개 source의 explicit storage를 factorized factory에 넣는 fixture 오류로
7/8이었으며, 제품 factory를 변경하지 않고 위의 실제 `P1P3P3` ordinal 연결로
고쳤다. 실제 Full P1P7P7의 negative/positive/negative와 수정된 108개 원본 전수
대조를 모두 실행했다. 최종 로그는
[exhaustive-d9225aa1-factorized-source.log](../../_local/artifacts/full6l-p1p7p7-20261122/exhaustive-d9225aa1-factorized-source.log)다.

별도로 `e6301cdb`의 실제 factorized P6(5,040개 원본, 6 locks)의 serial/parallel
결과에서 전체 normalized identity 배열, 전체 원본 bitmap, count/hash, raw candidate
count/digest와 family count가 일치했다. consumed-candidate 취소 후 새 control로
재개해도 count/probability 완료 권한을 얻지 못하는 회귀도 통과했다. 이 검증들은
Full 6L 전체 후보를 완주했다는 뜻이 아니다.

`fb9ab33a`의 footprint reuse 회귀는 **3/3**, candidate projection 회귀는
**3/3 통과**했다. 위의 실제 15-operation 혼합 Full 6L fixture와 원본 source
교집합 비교를 포함한다. 로그는
[reuse-fb9ab33a-footprint.log](../../_local/artifacts/full6l-p1p7p7-20261122/reuse-fb9ab33a-footprint.log),
[reuse-fb9ab33a-projection.log](../../_local/artifacts/full6l-p1p7p7-20261122/reuse-fb9ab33a-projection.log)다.
이 footprint/projection 실행은 partial-prefix pruning의 별도 실행 검증이나
Full 6L 전체 완주를 대신하지 않는다.

이후 `0fbec64f`의 debug test binary를 33.74초에 빌드했고 `physical_prefix`
필터 **14/14가 0.88초에 통과**했다. 구성은 새 closure helper 2개, traversal 구조
5개, 물리 witness 3개, 실제 Full6 gate 및 전체 결과 통합 2개, 기존 observation
policy 회귀 2개다. mixed15의 32,768개 subsets와 역순, 실제 gap ILC 양성 및 순환
음성이 포함된다. 4-O 작은 실제 입력의 raw/filtered × serial/parallel에서 전체
normalized identity/key/hash, 원본 bitmap, 대표 path, raw family count 및 최종
`raw − emitted`도 비교했다. 이 결과는 prefix pruning의 실행 검증이며, 미구현
RowOrder 표의 검증이나 원본 Full6 전체 완주를 뜻하지 않는다. 로그는
[prefix-0fbec64f-unit-build.log](../../_local/artifacts/full6l-p1p7p7-20261122/prefix-0fbec64f-unit-build.log),
[prefix-0fbec64f-focused.log](../../_local/artifacts/full6l-p1p7p7-20261122/prefix-0fbec64f-focused.log)다.

최초 `785baa31` 실행의 13/14 통과와 1개 실패 로그도 보존했다. negative traversal의
large fixture가 `Union(dead, dead)`의 동일-child 축약으로 커지지 않아 예상한
중간 yield에 도달하지 않은 테스트 구성 오류였다. fixture만 보정했고 제품의
closure 판정이나 완료 조건을 바꾸지 않았다. 원래 기록은
[prefix-785baa31-focused.log](../../_local/artifacts/full6l-p1p7p7-20261122/prefix-785baa31-focused.log)다.

후속 최적화는 exact candidate set과 원본 큐 교집합을 보존해야 한다. 정적 closure
stall, 완결된 물리 언어의 공집합, 완결된 source product의 공집합만 해당 범위의
음성 증거로 사용한다. 취소, 자원 거절, 인증 fallback 또는 coverage 조기 포화는
전체 해답 영역 소진의 증거로 사용하지 않는다.

# Symbolic family: exact-cover 호환 필터, 공유 병렬 전이, CTK4 저장

작성일: 2026-11-23. 이 문서는 현재 작업트리의 구현, 확인된 유한 검증, 세 shipping
Full4 실행의 미완료 관측을 정리한다. Row-prefix projection과 semijoin을 추가하기
전후의 실행, 별도 CTK4 native 왕복 검증, 완료된 piece의 cache 수명 변경과 그
검증·실제 실행을 구별한다. 최종 후보는 rank 2를 완료했지만 rank 3의 이동 계산
중 종료되었다. Full4 완료와 60초 미만 조건을 충족하지 못해 Full6는 실행하지 않았다.
유한 correctness fixture의 통과를 Full4 완료나 Full6 성능으로 해석하지 않는다.

이번 변경은 완성 cover를 하나씩 꺼내 Build로 검증하는 경로를 추가하지 않는다.
현재 symbolic 결과 관계를 유지하면서 기하적 확장 가능성의 계산과 중복 저장을
줄이고, 결과도 공유 DAG로 저장한다. 제품 기본 dispatch는 바꾸지 않는다.
CI 실행·조회·대기는 현재 승인 범위에 없으며, 알려진 100% 확률 명제의 추가 검색,
탐색, 증명은 수행하지 않는다. 다음 Full6 시도는 Full4 P7P4의 실제 개선을 확인한
뒤에만 판단한다.

## 1. 변경 범위

| 영역 | 구현된 변경 | 남는 경계 |
| --- | --- | --- |
| 기하 생성 | 기존 residual memo와 component 공유를 symbolic 호출에서도 사용 | 생성 자체의 시간·메모리 측정 필요 |
| 부분 배치 필터 | Row-prefix projection에 원본 Append/Union/Product 호환 조건 적용, 원본 joint relation으로 semijoin | 원본 geometry는 고정되어도 row family·joint ZDD·memo는 커질 수 있음 |
| 계산 cache 수명 | 한 piece의 모든 row·supply-state 소비 뒤 compatibility memo 두 개 해제 | 다음 piece의 일부 계산을 다시 수행할 수 있으며, active piece의 working set은 남음 |
| 이동 계산 | 같은 rank의 immutable Boolean prefix를 공유하는 piece별 worker | joint ZDD 변경과 row별 합성은 coordinator가 담당 |
| 결과 저장 | 세 root가 공유하는 노드를 CTK4로 저장 | 원본 source universe 전체·query 인증은 별도 범위 |
| 결과 읽기 | 검증된 DAG의 요청 page만 추출; 필요하면 해당 page만 CTK3 변환 | unknown file은 먼저 전체 graph 검증 필요 |

앞선 설계와 미완료 Full4 실행 관측은
[direct family 설계](full6l-direct-family-evaluation-2026-11-23.md),
[초기 구현 기록](direct-symbolic-family-implementation-2026-11-23.md),
[Full4 비교 기록](full4-symbolic-family-comparison-2026-11-23.md)에 보존한다.
아래 설명은 그 기록의 실패를 성공으로 바꾸지 않고, 이후 코드 변경을 구분한다.

## 2. Exact-cover 조건을 원본 표현에 직접 적용

### 2.1 셀 비트와 row-ID

요구 셀 집합을 비트마스크 \(R\), catalog row-ID 집합을 \(V\), row \(r\)의
원래 셀 footprint를 \(M_r\)라 하자. 여기서 row는 보드의 가로줄이 아니라
**미노 종류와 원래 footprint를 식별하는 placement 항목**이다.

이 필터가 받는 완성 family \(F\)의 각 원소 \(C\)는 같은 \(R\)을 겹침 없이
정확히 덮는다. 모든 catalog footprint는 비어 있지 않고 \(R\) 안에 있어야 한다.
현재 구현은 이 조건을 추측하지 않고 `certify_exact_cover`에서 검사한다.
[구현: geometry_compatibility_filter.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_compatibility_filter.rs)

선택된 부분 배치 집합 \(S\)에 대해 다음 guard를 사용한다.

\[
G_r(S)=\bigwedge_{\substack{t\in V,\ t\ne r\\M_t\cap M_r\ne\varnothing}}
\neg[t\in S].
\]

이는 \(r\) 자신을 금지하지 않고, \(r\)과 셀이 겹치는 **다른 row-ID**의 선택을
금지한다. 같은 footprint를 가진 다른 catalog 항목도 구별한다. 한 complete cover
\(C\)에 대한 호환 조건과 전체 family의 조건을 다음과 같이 둔다.

\[
K_C(S)=\bigwedge_{r\in C}G_r(S),\qquad
K_F(S)=\bigvee_{C\in F}K_C(S).
\]

검사된 exact-cover 범위에서는

\[
K_F(S)\iff \exists C\in F:S\subseteq C
\]

가 성립한다. \(S\subseteq C\)이면 complete cover 안의 서로 다른 footprint가
겹치지 않아 모든 guard를 통과한다. 반대로 \(S\)에 \(C\)에 없는 row \(t\)가
있다면, 비어 있지 않은 \(M_t\subseteq R\)의 어느 셀을 \(C\)의 다른 row가
덮으므로 해당 guard가 실패한다. 이는 구현의 부분집합 판정 불변식이며, 큐의 확률
명제를 다시 증명하는 작업이 아니다.

### 2.2 세 연산자와 local predicate

원본 geometry expression을 완성 cover나 새 derivative family로 펼칠 필요 없이
다음 연산을 적용할 수 있다.

| 원본 표현 | 호환 predicate | joint family \(J\)에 적용하는 방식 |
| --- | --- | --- |
| `Append(r,A)` | \(G_r\land K_A\) | \(J\)를 guard로 제한한 뒤 같은 결과에 자식 필터 적용 |
| `Union(A,B)` | \(K_A\lor K_B\) | 왼쪽 결과 \(L\)을 얻고, \(J\setminus L\)에만 오른쪽 필터를 적용한 뒤 union |
| `Product(A,B)` | \(K_A\land K_B\) | \(J\)를 A로 제한하고 그 결과를 B로 다시 제한 |

`FAMILY_INVALID`는 해가 없는 family이므로 false다. `FAMILY_EMPTY`는 빈 cover
하나를 뜻하며, 그 **local compatibility predicate는 true**다.

특히 자식의 \(K_A\)를 자식 family의 `Down(A)`와 혼동하면 안 된다. 자식이
담당하는 셀 영역 밖의 선택은 다른 Product 자식이나 상위 Append가 검사한다.
빈 영역의 자식이 전역 \(S\)를 빈 집합으로 제한하면 올바른 부분 선택까지 사라진다.
전체 root의 영역이 \(R\)을 덮을 때에만 root predicate가 위의 `Down(F)` membership와
같아진다. [연산자 구현과 terminal 처리](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_compatibility_filter.rs)

인증 단계는 node 순서와 row 범위를 확인하면서 각 자식의 셀 영역을 계산한다.
Append에서는 footprint가 자식 영역과 겹치지 않아야 하고, Union은 두 자식의
영역이 같아야 하며, Product는 두 영역이 겹치지 않아야 한다. Root 영역은 요구
셀과 같아야 한다. 이 검사는 원본 geometry node 수와 catalog 크기에 비례하는
순회이며, complete cover 수에 비례하는 순회가 아니다.

### 2.3 원본 큐와의 상관을 보존하는 필터

실제 입력은 \(S\)만의 family가 아니라 원본 큐 \(q\)를 붙인 joint relation
\(J\subseteq\{(S,q)\}\)다. 필터는

\[
T_A(J)=\{(S,q)\in J:K_A(S)\}
\]

를 계산한다. Product는 \(T_B(T_A(J))\)로 처리하며, 서로 다른 \(q\)에서 얻은
독립 projection을 곱하지 않는다. Guard는 catalog 변수만 검사하고, 순서상 뒤에
있는 source 변수 suffix를 그대로 유지한다. 따라서 여러 원본 큐가 같은 부분
배치를 공유해도 그 관계가 사라지지 않는다.

Union에서는 \(L=T_A(J)\)를 먼저 구한 뒤

\[
T_{A\cup B}(J)=L\cup T_B(J\setminus L)
\]

로 계산한다. 이미 왼쪽 조건을 통과한 원소는 오른쪽 조건을 다시 검사할 필요가
없다. 이는 같은 joint 원소에 대한 pointwise restriction이므로 source 상관도
유지한다. \(L=J\)이면 오른쪽 전체를 생략하고, \(L=\varnothing\)이면 difference
없이 원래 \(J\)를 오른쪽에 전달한다. Canonical root equality를 사용하므로
추정 count나 hash 일치에 근거한 생략이 아니다.
[필터 및 owner/epoch 검사](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_compatibility_filter.rs)

차집합 자체도 ZDD apply이며 비용이 있다. `union_remainder_calls`,
`union_remainder_created_nodes`, `union_remainder_work_steps`로 그 추가 비용을
기록한다. 실행 보고서의 필드에는 `compatibility_` 접두사가 붙는다. 오른쪽에서
줄어드는 중복 계산과 차집합 비용의 실제 관계는 후속 실행에서 확인해야 한다.

### 2.4 Source suffix를 제외한 계산과 정확한 semijoin

첫 shipping Full4 실행에서는 새 geometry derivative node가 0개인 상태에서도
호환 memo와 joint ZDD가 크게 증가했다. 이후 `row_down` 경로는 guard가 source
변수에 의존하지 않는다는 사실을 이용해 입력을 먼저 줄이도록 변경했다. 한 supply
state에서 row를 추가한 joint relation을 \(J\), 전체 root의 exact compatibility
predicate를 \(D\)라 두면 다음과 같이 계산한다.

\[
P(J)=\{S:\exists q\ (S,q)\in J\},\qquad
A=\{S\in P(J):D(S)\},
\]

\[
J\ltimes A=\{(S,q)\in J:S\in A\}
           =\{(S,q)\in J:D(S)\}.
\]

이는 다른 큐에서 나온 독립 결과를 결합하는 연산이 아니다. 마지막 단계는 **원래
\(J\)의 원소만 남기는 제한**이며, 같은 row set을 만족한 실제 source suffix를
그대로 보존한다. Supply/Hold 상태별 root 분리도 유지한다. \(A=P(J)\)이면
원래 \(J\) 전체가 통과하므로 semijoin도 생략한다.

현재 variable 순서는 row-ID가 `[0, row_count)`, source가 그 뒤다.
`project_prefix(J, row_count)`는 row node만 순회하고, source 영역에서 처음 만난
**비어 있지 않은 suffix를 `UNIT`**으로 바꾼다. `EMPTY`는 계속 `EMPTY`다. 따라서
source의 모든 node나 queue member를 순회할 필요가 없고, count가 `u128`을 넘는
suffix도 여기서 count를 계산하지 않는다.

`filter_prefix(J, A, row_count)`는 두 root의 row 변수 선택 여부를 함께 순회한다.
허용 row prefix를 다 확인하면 원본 joint suffix ID를 그대로 반환한다. 여기서
`UNIT`은 `{∅}`이며 Boolean true가 아니다. 허용 family가 `UNIT`이어도 joint에
row 변수가 남았다면 그 변수를 선택하지 않는 low 경로를 계속 확인해야 한다.
[Prefix projection과 semijoin](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_partial_family.rs),
[row_down 통합](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family.rs)

Compatibility memo의 구조는 `(geometry node, input root)` 및 `(guard row, input
root)`이며, 이 경로의 input root는 이제 source를 포함하지 않는 row family다.
같은 부분 배치에 다른 source suffix가 붙어 있다는 이유만으로 geometry 조건을
다시 계산하지 않도록 한 것이다. Arena owner token과 compaction epoch를 확인해
다른 arena나 compact 전의 숫자 ID를 재사용하지 않는다. Projection memo에는 cut이
포함되며, semijoin의 `(joint, allowed)` pair memo는 호출마다 새로 만들어 고정 cut과
함께 사용한다. 두 root의 순서는 바꾸지 않는다.

이 경로는 새 geometry derivative node를 만들지 않지만 row family를 만드는 비용,
semijoin apply, 임시 node와 memo 비용은 남는다. 총 비용을
`O(geometry nodes + joint nodes)`라고 주장하지 않는다. Explicit node/work/memo
bounds와 취소는 그대로 전파하며 거절을 빈 해집합으로 바꾸지 않는다. Rank
compaction 전에 memo를 비우고, 마지막 result compaction 전에 geometry·predicate·
bridge owner를 해제하는 수명 조정도 함께 적용했다.
[Rank와 결과 수명](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family.rs)

### 2.5 완료된 piece에서 파생 cache 해제

최종 후보는 rank 전체가 끝날 때까지 compatibility memo를 유지하던 수명을
짧게 한다. Coordinator의 `consume_piece`가 해당 piece의 모든 legal row와 준비된
supply state를 처리해 `next` root에 합친 뒤, `filtered`와 `guarded` map을 새 빈 map으로
교체한다. 같은 piece 안에서는 기존 계산을 계속 재사용한다. 병렬 worker 결과를
소비하는 경로와 명시적 bounds 등에 따른 순차 경로가 같은 소비 함수를 사용한다.
[완료 경계](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family.rs),
[clear_memo](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_compatibility_filter.rs)

해제하는 것은 다시 계산 가능한 파생 map과 그 owner/epoch 참조다. Immutable geometry,
ZDD node와 현재·다음 rank의 joint roots는 유지한다. 다음 piece가 이전과 같은 조건을
물으면 기존 정의로 정확히 다시 계산한다. Work budget, 누적 work·miss counter,
취소 상태는 초기화하지 않으므로 cache 해제가 명시적 제한을 갱신하는 수단이 되지
않는다. Rank compaction 앞의 별도 memo 해제도 유지한다.

메모리 감소와 재계산 사이의 비용을 확인할 수 있도록 두 실행기의 rank 보고에
다음 필드를 포함한다.

| Rank 필드 | 의미 |
| --- | --- |
| `compatibility_cache_bytes_estimate` | 완료된 piece들의 map 해제 뒤 rank 말에 남은 backing capacity 추정. `row_cells` 등은 남으므로 반드시 0이 되는 값이 아님 |
| `compatibility_cache_peak_bytes_estimate` | Rank 시작과 완료된 piece의 **해제 직전 경계**에서 관측한 retained capacity의 최댓값 |
| `compatibility_cache_releases` | 한 rank에서 piece 소비를 마친 뒤 수행한 memo 해제 횟수. 마지막 rank-compaction 해제는 이 횟수에 포함하지 않음 |
| `compatibility_filter_misses` | 해당 rank의 누적 filter miss 증가량. Piece별 해제를 거쳐도 유지 |
| `compatibility_guard_misses` | 해당 rank의 누적 guard miss 증가량. Piece별 해제를 거쳐도 유지 |

Peak 필드는 active piece 내부의 모든 순간을 샘플링한 값이 아니다. Allocator overhead,
rehash 중의 일시적 메모리, process RSS도 포함하지 않는다. 실제 해제 뒤에는 progress의
compatibility memo entry 수를 0으로 갱신하고 남은 capacity 추정을 다시 보고한다.
이 정책으로 다른 piece에서 재사용하던 일부 답을 잃을 수 있으므로 work/miss 증가량과
실제 실행 메모리를 함께 확인해야 한다. 한 piece 자체가 큰 경우나 ZDD·Boolean
node가 커지는 경우의 비용을 이 해제만으로 제한하지 않는다.
[일반 실행기 보고](../../crates/clearra-core-executor/examples/symbolic_family.rs),
[비교 실행기 보고](../../crates/clearra-core-executor/examples/symbolic_family_compare.rs)

## 3. 기존 geometry 계산의 재사용 복원

기존 `FamilyCompiler::try_new`는 resource-authoritative 호출을 위한 생성자로,
residual memo와 inventory 사용을 끄고 advanced component composition도 제한했다.
Symbolic API의 제한은 명시적 logical node/work bounds이므로 이 생성자 선택이
무제한 symbolic 호출에서도 기존 계산 재사용을 끄는 결과를 만들었다.

현재 `compile_symbolic_geometry`는 checked prefix/stack 준비를 유지하면서 일반
compiler와 같은 residual memo와 component 재사용 경로를 복원한다. 정렬·중복 제거된
piece multiset target을 받아 처리하며, 입력 문자열 `P7P4`나 특정 kick 이름으로
분기하지 않는다. 기존 `TwoBagPlusOneInventory`가 target 조건을 만족할 때 선택되는
구조는 재사용하지만, 이번 변경에서 그 이름의 큐만을 위한 새 알고리즘을 추가한
것은 아니다. 조건을 만족하지 않는 target도 일반 exact compiler 경로를 사용한다.
[geometry.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs)

이 API는 원본 compressed expression만 반환한다. Complete-candidate enumerator,
completion-oracle traversal marks, 전체 path-count table을 생성해서 넘기지 않는다.
생성이 끝나면 immutable node 참조에 필요 없는 construction interner를 해제할 수
있도록 했다. 이 해제량은 backing capacity 추정치이며 OS RSS 감소량과 같지 않다.
[geometry_family.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_family.rs)

## 4. 같은 rank의 이동 계산을 공유하며 병렬화

### 4.1 공유하는 계산과 단일 변경 owner

Rank마다 현재 joint relation에서 source 변수를 존재적으로 투영해 물리 domain을
한 번 만들고, 그 domain에 대한 occupancy·line-clear 표현을 준비한다. Source/Hold
전이는 piece 종류와 supply state별로 한 번 계산한다. 각 piece의 이동 관계와
legal-row predicate 계산을 병렬 task로 배분한다.
[Rank 실행](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family.rs),
[domain/관계 bridge](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_bridge.rs)

`freeze_prefix`는 Boolean node 배열, unique table, apply table을 복제하지 않고
한 `Arc<FrozenBooleanPrefix>`로 옮긴다. Worker는 이 공통 prefix를 읽고 자신의 새
node와 interning/memo suffix만 소유한다. Prefix의 동일 owner는 읽을 수 있지만,
같은 숫자 local ID를 가진 sibling worker의 suffix를 자신의 관계로 해석할 수 없다.
[symbolic_boolean.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_boolean.rs)

Coordinator만 joint ZDD를 변경한다. 완성된 worker의 Boolean arena를 빌려
`FamilyBridge`에 직접 전달하고, source 조건과 legal predicate를 결합한 후 row 추가,
기하 호환 필터, 다음 supply state별 union을 수행한다. Worker의 local node를 공통
arena에 모두 복사하는 단계는 없다. 모든 결과 소비와 worker 종료 뒤 prefix를 원래
owner에 되돌려 다음 rank의 rewind를 수행한다.
[symbolic_family_parallel.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_parallel.rs)

### 4.2 시작 정책, bounds, 메모리 해석

생성된 worker는 다른 worker 전체가 준비될 때까지 기다리는 barrier 없이 task를
즉시 가져간다. Worker 수의 task-count 제한은 실제 eligible piece 종류 수이며,
새 논리 CPU 상한을 만들지 않는다. 현재 작업 단위가 종류별이므로 동시에 배분할 수
있는 독립 task는 최대 일곱 개다. `requested_workers`, `spawned_workers`, 실제 task를
수행한 `active_workers`, 동시 movement task peak를 구분해서 보고한다.

| 조건 | 실행 경로 |
| --- | --- |
| Native parallel feature와 복수 worker·복수 eligible piece, Boolean bounds 모두 0 | Piece movement 병렬 경로 |
| 명시적 Boolean node/work/memo bound | 해당 bounds의 의미를 유지하는 순차 경로 |
| 한 worker 또는 한 종류 이하의 task | 순차 경로 |
| Native parallel feature 사용 불가 | 순차 경로 |
| Spawn/worker/consumer 실패 또는 취소 | Worker를 종료·회수하고 오류 반환; 다른 탐색기로 성공 대체하지 않음 |

결과 전달은 용량 0의 rendezvous channel을 사용한다. 이는 coordinator가 소비하지
않은 결과를 추가 queue에 쌓지 않는 backpressure다. 전송에서 기다리는 worker도
자신의 suffix arena를 계속 소유하므로, 이를 전체 working-set 상한으로 해석하면
안 된다. 공유 prefix는 한 번만 계상하고, private peak의 합계는 보수적 합산값으로
보고하며 실제 동시 RSS와 구분한다.

Joint ZDD apply, compatibility filtering, rank compaction은 여전히 단일 owner의
작업이다. 이번 구현만으로 모든 직렬 계산이나 메모리 병목이 없어졌다고 주장하지
않는다. NUMA별 배치·pinning이나 단일 NUMA/다중 NUMA 속도 개선도 이 변경의
검증 결과에 포함되지 않는다.

### 4.3 Kick과 line-clear 의미

기하 호환 필터와 병렬 owner 구조는 특정 kick profile을 구분하지 않는다. 이동
관계는 기존 active ordered-kick authority와 line-clear frame 표현을 사용한다.
SRS, SRS+, SRS-X, Jstris 180, no-kick의 유한 기존 solver 대조 fixture가 통합 검사에
포함되어 있다. Imported ordered tables의 실행 경로를 재사용하는 것과 가능한
모든 사용자 table을 테스트했다는 주장은 구별한다.
[이동 표현](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_family.rs),
[통합 fixture](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_tests.rs)

## 5. 완성 해법 열거 없는 CTK4 저장

### 5.1 CTK3와 CTK4의 범위

CTK3는 이미 선택된 colored page와 comment/operation을 저장한다. Segment 단위
decode는 가능하지만 ZDD의 대안, 공유 suffix, identity/source joint relation을 직접
보존하는 container가 아니다. 따라서 전체 해법을 CTK3로 변환하는 것을 결과 저장의
필수 단계로 삼지 않는다.

새 `clearra-ctk4`는 **experimental ordered-ZDD solution-family dialect**다. Generic
codec은 board, queue, Hold, kick을 알지 못하며 atom dictionary와 application metadata를
opaque bytes로 취급한다. 원래 Append/Union/Product geometry를 export 과정에서
미리 ZDD로 전개하는 입력 경로도 제공하지 않는다.
[CTK4 contract](../../crates/clearra-ctk4/README.md),
[CTK3 contract](../ctk3.md)

| 저장 항목 | 의미 |
| --- | --- |
| `accepted` root | 선택된 전체 placement와 원본 source의 joint relation |
| `identities` root | Source 변수를 투영한 placement set family |
| `covered_sources` root | Placement 변수를 투영한 covered source family |
| 공통 node table | 세 root가 같은 ordered-ZDD node를 공유 |
| Atom dictionary | 원래 placement의 piece/cells 또는 source position/piece |
| Scope metadata | Board width/height, initial/required cells, row count, 원래 source length |

Wire format은 `CTK4ZDD\0` magic, revision, roots와 block directory를 담은 manifest,
manifest SHA-256, 독립 checksum을 가진 atom/node blocks로 구성된다. Nonterminal은
`(variable:u32, low:u64, high:u64, count:u128)`의 36-byte record다. Counts는
overflow를 오류로 처리하며 포화값을 exact count로 내보내지 않는다.
[format.rs](../../crates/clearra-ctk4/src/format.rs)

Node의 모든 child ID는 parent보다 작고, nonterminal child의 변수는 더 크다.
High child는 공집합이 아니어야 한다. 이 조건에서 low/high 집합은 현재 변수의
선택 여부로 분리되므로 count는 두 child count의 합이다. BDD와 달리 low/high가
같은 ZDD node도 유효하다. 구조가 같은 node record의 중복은 허용하므로 archive
digest를 의미적으로 canonical한 solution-set digest와 혼동하지 않는다.

### 5.2 Writer와 검증 권한

`write_ctk4`는 세 root를 함께 compact한 뒤 retained node마다 16바이트의 임시
count table을 계산한다. Generic encoder는 node와 dictionary를 순회해 기록하고,
실제 기록된 파일을 다시 검증한 뒤 `ProducerSeal`을 반환한다. 이 과정은 node
단위이며 complete identity나 원본 queue 목록을 열거하지 않는다. 임시 count table,
compaction, 검증 I/O 비용은 solver search 시간과 별도다.
[Result adapter](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_archive.rs),
[encoder.rs](../../crates/clearra-ctk4/src/encoder.rs)

| Reader 상태 | 가능한 작업 | 권한을 얻는 조건 |
| --- | --- | --- |
| `UnverifiedArchive` | Manifest와 선언된 roots/count 확인 | Header·directory·manifest hash 검사 |
| `VerifiedArchive` | Exact atom-set count, membership, bounded page | 전체 node/atom 검증 또는 동일 artifact의 live producer seal |
| `SymbolicFamilyArchive` | Board/source schema 및 요청한 identity/source 해석 | Verified generic reader 위의 application schema 검사 |

Manifest hash만으로 count recurrence가 옳다는 결론을 내리지 않는다. Unknown file의
`verify_all`은 모든 actual node와 root count를 확인한다. Private `ProducerSeal`은
writer 또는 성공한 strict scan만 생성하며, 임의의 file digest를 읽어 재구성할 public
constructor/deserialize 경로가 없다. Seal은 같은 manifest digest와 archive length에
결박되고, lazy fetch 시에도 각 block checksum을 확인한다.
[reader.rs](../../crates/clearra-ctk4/src/reader.rs)

이 seal은 영구 서명이 아니다. 새 process에서 파일을 열면 다시 strict scan이
필요하다. Node마다 한 번씩 recurrence를 검사하는 logical scan이지만 bounded cache
때문에 이전 block을 재읽고 해시할 수 있어 총 I/O가 한 번의 순차 읽기라고 보장하지
않는다.

Generic 검증은 atom-ID 집합의 exactness를 보장한다. 외부 dictionary가 서로 다른
row-ID를 같은 placement에 매핑하는지, 모든 집합이 게임 규칙상 합법적인지, 세 root가
어떤 완전한 query의 projection인지까지 자동 인증하지 않는다. Adapter는 요청된
page의 placement 수·셀 범위·겹침·완성 cover를 검사하고, count API를
`identity_row_set_count`로 명명한다. 원본 request fingerprint는
`unverified_annotation`이며, 원래 uncovered source universe와 확률 분모의 권한도
이번 archive만으로 독립 복원하지 않는다.

### 5.3 Lazy page와 소유권

Host가 `ReadAt`을 구현해 저장소와 수명을 소유한다. Codec은 임의 경로나 URL을
열지 않고 필요한 byte range를 요청한다. Page cursor는 archive digest, root, `u128`
offset에 결박할 수 있다. Pager는 count를 이용해 요청 rank window와 교차하는 child만
schedule하며, 앞 identity를 열거하거나 무관한 sibling stack을 쌓지 않는다.
선택된 prefix와 반환 page는 명시적 representation bound를 적용받는다.

Archive 안의 주소는 frozen snapshot의 주소이며 live arena의 이후 compact와 연결되지
않는다. Cache도 reader별로 소유한다. Native host는 새 `.partial` 파일에 기록하고
sync한 뒤 같은 디렉터리의 hard link로 기존 출력을 덮지 않는 publication을 수행한다.
실패한 partial 파일을 성공 결과로 보고하지 않는다.
[Native archive boundary](../../crates/clearra-core-executor/examples/support/symbolic_archive.rs)

요청한 일부 identity는 기존 CTK3 encoder로 colored page로 바꿀 수 있다. 현재 별도
decoder는 canonical CTK1을 comment에 넣어 같은 색의 인접 미노 partition 정보를
남긴다. 이것은 요청한 page의 표시용 projection이며 전체 archive를 CTK3로 열거하는
작업이 아니다. Native range reader는 구현되었고, async browser/network reader와
제품 UI 연결은 별도 작업이다.
[독립 decoder](../../tools/clearra-symbolic-bench/src/bin/symbolic_ctk4.rs)

## 6. 실행 경로와 계측 계약

빌드는 `-p clearra-symbolic-bench --release --bins`로 package를 직접 선택한다.
Executor의 example/dev-dependency feature closure를 실행기 build에 끌어오지 않으면서
같은 command 소스를 재사용한다. 이 구조 변경은 workspace의 LTO/codegen 설정을
낮추는 변경이 아니다.
[실행 README](../../tools/clearra-symbolic-bench/README.md),
[빌드 진단](build-and-signal-diagnostics-2026-11-23.md)

Full4 P7P4는 `benchmark-search --no-timeout` 아래에서
`symbolic_family_compare --backend symbolic --archive-only`로 실행한다. Actual fresh
standard 7-bag 11 draws, 10 locks, empty enabled Hold, Jstris 180, FullQueueOracle,
CountUnique 조건을 유지한다. Archive-only는 complete identity·coverage bitmap
전개를 생략하며 summary의 projection/joint comparison 완료 여부를 true로 바꾸지
않는다. Search 반환 직후 checkpoint를 먼저 남기고 archive 시간을 별도 기록한다.
[비교 명령](../../crates/clearra-core-executor/examples/symbolic_family_compare.rs)

독립 `symbolic_ctk4 inspect`는 선언 정보만 읽는다. `verify`와 `page`는 각각 unknown
archive의 전체 검증을 수행한다. 별도 process에서 `verify` 후 `page`를 실행하면
검증은 두 번 일어난다. Page 명령은 검증 cache를 비운 후 요청 page의 추가 I/O를
별도 기록한다. Page 반환 뒤의 `additional_bytes_read`, `additional_block_fetches`,
`additional_cache_hits`, 현재 `retained_cache_bytes`·`uncached_block_bytes`,
`peak_page_stack_entries`도 page 항목에 기록한다. Page 전의 검증 통계만 보고해
실제 pager frontier가 누락되던 부분을 보완한 것으로, count/page 권한이나 decoder
동작은 바꾸지 않았다. `--ctk3-output`은 해당 bounded page만 출력한다.

Archive-only 완료는 요청한 DAG 저장 명령의 완료를 뜻한다. 전체 identity 또는
global source bitmap을 출력한 것은 아니므로 해당 export 완료 flag와
`joint_comparison_complete`는 false로 유지한다. 비교 모드는 양쪽 projection export
완료, checked source count, source/request/binary 지문을 확인하고 두 증거 파일의
전체 byte를 대조한다. 따라서 archive-only summary를 기존 전체 projection 비교의
성공 결과로 받아들이지 않는다.

Work pulse와 storage estimate는 계산 문맥을 설명하는 관측치다. Compatibility step이
ZDD checkpoint도 진행시키거나 worker work가 coordinator 보고에 합산되므로 서로
겹치는 counter를 모두 더해 전체 연산 수로 해석하지 않는다. Retained capacity,
private peak 합계, 실제 RSS/HWM도 서로 다른 값이다. 최신 phase/certificate/storage
telemetry를 포함한 release 실행기 빌드 성공과 실제 Full4 관측의 완료 여부는
아래 표에서 구별한다.

## 7. 확인된 검증과 남은 검증

아래는 root가 실행하고 결과 로그를 확인한 유한 correctness 검사다. 작은 입력의
성능 측정은 하지 않았고, test duration을 탐색 성능으로 사용하지 않는다. 원시 출력은
`_local/artifacts`에 두며 이 문서에는 검토한 결과만 기록한다.

| 검증 | 확인된 결과 | 범위와 로컬 근거 |
| --- | --- | --- |
| Compatibility algebra | 최신 4/4 통과 | Explicit correlated members·기존 Down·prefix/semijoin 경로 대조, Union remainder의 shared/exclusive source, exact-cover 인증 거절, 취소/bounds. `symbolic-prefix-compatibility-tests-20261123.log` |
| Symbolic integrated fixtures | Cache 수명 변경 뒤에도 29/29 통과 | Archive 5개, source/Hold/EOF, bridge 상관, 기존 solver 유한 대조, full-layer 병렬 대조, 5 builtin profile 및 imported ordered-table 구별 포함. `symbolic-piece-cache-tests-20261123.log` |
| ZDD/Down primitive fixtures | 최신 17/17 통과 | 기존 15개에 prefix projection/semijoin의 correlated suffix, source graph 생략과 refusal 검증 추가. `symbolic-prefix-zdd-tests-20261123.log` |
| Boolean primitive fixtures | 6/6 통과 | Boolean 의미, frozen prefix allocation 공유와 fork ID 범위, bounds/secondary cancellation, iterative apply와 rewind. `symbolic-boolean-final-tests-20261123.log` |
| Generic CTK4 | 16/16 통과 | Roundtrip, root 공유, malformed/cycle/order/count/hash/overflow, cancellation/bounds, lazy fetch, 깊은 singleton page frontier. `ctk4-lazy-window-tests-20261123.log` |
| 첫 shipping release 실행기 | 빌드 통과 | Prefix/semijoin 이전 `symbolic_family_compare`와 executor dependency를 기존 release 최적화로 컴파일. `symbolic-shipping-release-timings-20261123.log` |
| Prefix/semijoin 및 page telemetry 포함 세 실행기 | Release `--bins` 빌드 통과 | Tree `eb87b3b8cac7d31d5702adc61245f04548a396f3`; `symbolic_family`, `symbolic_family_compare`, `symbolic_ctk4`. `symbolic-prefix-shipping-release-20261123.log` |
| 완료 piece cache 수명 변경 후 세 실행기 | Release `--bins` 빌드 통과 | Tree `6bab3217ddcce8484f11685c3a1f96d4600cc809`; 관리 실행 시간 121.276초는 빌드 시간. `symbolic-piece-cache-shipping-release-20261123.log` |
| Native CTK4 저장·독립 decoder·CTK3 page | 앞선 후보에서 유한 왕복 검증 통과 | 4 identities / 1 source / 4 pairs, 요청한 3개 identity와 CTK3 comment·cells 일치. Archive/decoder 코드는 이후 변경하지 않았으며 이 native 검증은 재실행하지 않음. `ctk4-end-to-end-validation-20261123.json` |
| 첫 shipping Full4 P7P4 | 미완료, SIGKILL | Prefix/semijoin 이전 실행. Geometry 뒤 rank 2의 메모리 증가를 관측했으며 CTK4 없음. 아래 실행 표 참조 |
| Prefix/semijoin 이후 Full4 P7P4 | 미완료, SIGKILL | 두 번째 실행도 rank 2에서 종료. 서로 다른 piece/row 문맥의 관측이며 완료 시간·전체 RSS 개선으로 해석하지 않음 |
| 완료 piece cache 수명 변경 뒤 Full4 | Rank 2 완료 후 rank 3에서 SIGKILL | Full4 search 미완료, CTK4 없음. 44.694초는 종료까지의 프로세스 시간이며 성공 search 시간이 아님 |

새 `symbolic_family_parallel_layers_preserve_complete_identity_source_relation`은
앞선 23개 검사 뒤에 추가되어 24개 실행에서 통과했고, prefix 변경 후와 최종 cache
수명 변경 후의 각 29개 실행에도 포함되었다. 유한 fixed-source fixture의 전체
identity set, covered-source count, joint pair count와 identity별
source count를 병렬 symbolic과 explicit bounds를 둔 순차 symbolic 사이에 대조한다.
순차 reference는 같은 fixture의 기존 solver 결과와도 대조한다. 이 검사는 실제
병렬 worker가 생성되었는지와 bounded reference의 순차 fallback 이유도 확인한다.

새 prefix primitive 검사는 7개 변수의 유한 명시적 집합과 모든 cut에서 projection·
semijoin을 대조한다. 별도 fixture는 source 변수 5,000개의 압축 powerset suffix를
두고 suffix 순회·count 없이 원본 suffix를 보존하는지 확인한다. Work limit과
cancellation이 빈 해집합으로 바뀌지 않는지, 실패 뒤 scratch 계수가 회복되는지도
확인한다. 이는 실제 Full4의 메모리·속도 측정과는 다른 correctness 검사다.

확인한 관리 receipt는 다음과 같다.

| 실행 | Receipt ID |
| --- | --- |
| Prefix 변경 후 통합 29개 | `1795426173805893228-3` |
| Cache 수명 변경 후 통합 29개 | `1795426888342226403-3` |
| ZDD/Down 17개 | `1795426110429904721-3` |
| Boolean 6개 | `1795425494010156679-3` |
| Compatibility 4개 | `1795426180580394784-3` |
| CTK4 16개 | `1795425131853543058-3` |
| 첫 shipping release 실행기 | `1795425504244288723-3` |
| 첫 shipping Full4 P7P4 | `1795425735355417354-3` |
| Prefix/semijoin shipping 세 실행기 | `1795426240980932984-3` |
| Prefix/semijoin Full4 P7P4 | `1795426382130762307-3` |
| Cache 수명 변경 후 shipping 세 실행기 | `1795426936928146498-3` |
| Cache 수명 변경 후 Full4 P7P4 | `1795427063731448912-3` |

실제 fixture 근거는
[compatibility tests](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_compatibility_filter_tests.rs),
[archive tests](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_archive_tests.rs),
[codec tests](../../crates/clearra-ctk4/src/tests.rs),
[ZDD/prefix tests](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_partial_family_tests.rs),
[parallel movement tests](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_parallel.rs),
[full-layer 통합 tests](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_tests.rs)에 있다.

### 7.1 Native 저장과 독립 decoder의 왕복 검증

`symbolic_family`로 empty 2L, fixed `IIOOO`, enabled Hold, initial Hold 없음,
Jstris 180 조건을 실행하고 CTK4를 기록했다. 이어 별도 `symbolic_ctk4` process의
`inspect`와 `page --offset 1 --limit 3`를 실행했다. Rust helper는 live result의 요청
page와 독립 decoder의 canonical identity를 대조하고, 출력 CTK3를 기존 decoder로
다시 읽어 comment와 모든 colored cell을 대조했다.

| 확인 항목 | 결과 |
| --- | --- |
| 완성 identity / covered source / joint pair | 4 / 1 / 4 |
| 저장 단계 complete-solution 열거 | 0 |
| 공유 archive | 39 nodes, 4,172 bytes |
| 같은 writer의 seal로 lazy reopen | Data block fetch 0 |
| 독립 `inspect` | Data block fetch 0, `graph_verified=false` |
| 독립 `page`의 unknown archive 검증 | 전체 graph 검증 후 `graph_verified=true` |
| 요청 offset / limit / 반환 | 1 / 3 / 3 |
| Live page와 독립 page | Canonical identity 3개 모두 일치 |
| CTK3 표시 projection | Comment와 colored cells 3개 모두 일치 |
| 검증 cache를 비운 뒤 page의 추가 I/O | 2 block fetches, 2,972 bytes |
| Page frontier peak | `peak_page_stack_entries=3` |

검토한 근거는 `_local/artifacts/ctk4-end-to-end-validation-20261123.json`과 그 Rust
검산 helper다. 이 검증은 실제 native 실행·저장·독립 읽기 경계를 확인한다. 저장 시
전체 해법을 열거하지 않았다는 사실과, 이후 요청 page를 유한하게 추출했다는 사실을
구분한다. 이 fixture의 짧은 시간 값은 작은 입력 성능 benchmark로 보고하지 않는다.
Full4 해법 집합이나 모든 source joint 관계를 검산했다는 의미도 아니다.
이 native 왕복 검증은 prefix/semijoin 후보의 실행 기록이다. 이후 완료 piece cache
수명 변경은 archive/decoder 코드를 바꾸지 않았으며, 위 왕복 검증을 다시 실행한
것으로 표기하지 않는다.

## 8. 실제 실행 관측과 다음 비교

### 8.1 첫 shipping Full4: 생성 뒤 rank 2에서 미완료

이 실행은 2.4절의 prefix projection/semijoin과 Union remainder **이전 snapshot**이다.
실행 prefix는 `_local/artifacts/full4-symbolic-compatibility-20261123`이며,
`.observation.json`, `.summary.json`, `.runtime.json`의 선택된 관측을 아래에 기록한다.
실제 요청은 empty 4L, Jstris 180, fresh standard 7-bag의 11 draws, 10 locks,
empty enabled Hold이며, compiler가 가진 원본 source count는 4,233,600이다.

| 관측 | 확인된 값 | 해석 범위 |
| --- | --- | --- |
| Geometry compiler 완료 | Search 시작 뒤 1.082228605초 | Structured boundary의 실제 측정 |
| Compatibility certificate | 0.002094247초 | Geometry와 인증을 합한 boundary는 1.084322852초 |
| 프로세스 실행 시간 | 34.771초 | SIGKILL까지의 시간이며 완료 search 시간이 아님 |
| 마지막 계산 문맥 | `row_down`, rank 2/10, I, row 21/764 | 마지막 독립 sample은 harness 시작 뒤 34.023342866초 |
| 마지막 compatibility memo | 52,402,493 entries | Backing capacity 추정 763,369,440 bytes |
| 마지막 joint ZDD | 9,426,301 nonterminal nodes | Backing capacity 추정 756,580,736 bytes |
| 추가 geometry derivative nodes | 0 | Geometry node 증가가 없어도 다른 관계와 memo가 커질 수 있음 |
| Process sampled RSS/HWM peak | 2,768,936,960 bytes | Sampler가 확인한 process 값; 종료 직전의 절대 peak를 보장하지 않음 |
| Manager sampled peak | 2,796,679,168 bytes | Manager 측정 범위의 값이며 위 backing estimate와 같지 않음 |
| 종료 | Signal 9, exit 137 | `search_complete=false`, CTK4 저장 결과 없음 |
| 시간 제한·재시도 | `timeout_seconds=0`, automatic retry 없음 | Manager가 삽입한 deadline 종료가 아님 |

시작 시 물리 메모리는 총 10,451,423,232 bytes, available 4,288,618,496 bytes였고,
마지막 available 관측은 1,238,081,536 bytes였다. 메모리 증가와 SIGKILL은 메모리
부족을 조사할 근거다. 다만 관측 가능한 cgroup의 finite limit과 OOM-kill delta가
없으므로 이 receipt만으로 특정 종료 주체를 확정하지 않는다. Manager의 사전 종료
시도는 없었고, child 종료 확인 뒤 containment cleanup 시도만 남아 있다.

이 관측이 뒷받침하는 개선 지점은 생성 비용보다 생성 이후 호환 계산의 중복이다.
따라서 후속 변경은 joint source suffix를 geometry memo에 반복 반영하지 않는
projection·semijoin과 Union 잔여 집합 계산에 집중했다. 변경 뒤 두 번째 실행의
부분 관측은 다음 절에 별도로 기록한다.

| 실행 식별자 | 값 |
| --- | --- |
| Compiled source commit | `1fe73ed88fa350ef0ebd9ae9e17281061fc2ff54` |
| Compiled source tree | `81faa07b0d8cb140813ecc1c12782571e6d40cda` |
| Binary SHA-256 | `2742a04a5adcc7d90dff474f3fa4cf247a09a17ea1042fd1abdca96cefadbfd3` |
| Release profile note | `repository-release-thin-lto-cgu1-default-incremental` |
| Request fingerprint SHA-256 | `e709af430f47ba7292eba224b3af7414efaa13560082dc77c474089992afa924` |
| Source fingerprint SHA-256 | `989d0aa152b2a75913d02142fcca6df8d6e88302de116c84060fb22decc81d56` |

### 8.2 Prefix/semijoin 이후 두 번째 Full4도 rank 2에서 미완료

실행 prefix는 `_local/artifacts/full4-symbolic-prefix-20261123`이며,
`.observation.json`과 그 summary·receipt를 확인했다. Source/request fingerprint와
release profile은 첫 실행과 같다. 새 세 실행기의 shipping build는 통과했으며,
tree와 binary 지문은 아래와 같이 달라졌다.

| 실행 식별자 | 값 |
| --- | --- |
| Compiled source commit | `1fe73ed88fa350ef0ebd9ae9e17281061fc2ff54` |
| Compiled source tree | `eb87b3b8cac7d31d5702adc61245f04548a396f3` |
| Binary SHA-256 | `27251af6cc3a03643ec0085fd738c3852e658c99242710cecd664dcfa475635f` |
| Request fingerprint SHA-256 | `e709af430f47ba7292eba224b3af7414efaa13560082dc77c474089992afa924` |
| Source fingerprint SHA-256 | `989d0aa152b2a75913d02142fcca6df8d6e88302de116c84060fb22decc81d56` |

| 관측 | 확인된 값 | 해석 범위 |
| --- | --- | --- |
| Geometry compiler 완료 | Search 시작 뒤 0.893706301초 | Structured boundary의 실제 측정 |
| Compatibility certificate | 0.002189804초 | 합친 boundary는 0.895896105초 |
| 프로세스 실행 시간 | 34.755초 | SIGKILL까지의 시간이며 완료 search 시간이 아님 |
| 마지막 계산 문맥 | `row_down`, rank 2/10, J, row 577/764 | 마지막 독립 sample은 harness 시작 뒤 34.033166792초 |
| 마지막 compatibility memo | 65,806,082 entries | Backing capacity 추정 858,789,856 bytes |
| 마지막 joint ZDD | 2,035,494 nonterminal nodes | Backing capacity 추정 193,462,656 bytes |
| 추가 geometry derivative nodes | 0 | 기존 immutable geometry 표현 유지 |
| Process sampled RSS/HWM peak | 2,877,046,784 bytes | Sample로 확인한 process 값 |
| Manager sampled peak | 2,905,223,168 bytes | Manager 측정 범위의 값 |
| 종료·출력 | Signal 9, exit 137, search 미완료 | CTK4 저장 결과 없음 |
| 시간 제한·재시도 | `timeout_seconds=0`, automatic retry 없음 | Supervisor 종료 이유와 사전 종료 시도 없음 |

두 번째 마지막 관측에서 joint ZDD의 node 수와 backing estimate는 첫 관측보다
작았다. 그러나 호환 memo는 더 컸으며 전체 RSS가 감소한 관측도 아니다. 병렬
movement 결과를 받아 처리하는 piece 순서는 고정되어 있지 않고, 마지막 문맥도
I/row 21과 J/row 577로 다르다. Row-ID 크기는 완료한 작업량의 순서가 아니다.
따라서 이 두 부분 관측으로 처리량, 완료 시간, 전체 peak RSS 개선을 계산하지
않는다. Prefix 분리 뒤에도 compatibility cache의 수명이 남은 병목이라는
진단 근거로 사용한다.

이 관측 뒤 **처리가 끝난 piece의 계산 cache를 더 이른 시점에 해제하는 변경**을
통합했다. 동작과 계측 필드는 2.5절에 정리했다. 해당 코드의 통합 검사 29/29와 세 bin
shipping build가 통과했으며, 변경 뒤 실제 실행은 다음 절에 별도 기록한다.

### 8.3 최종 후보: rank 2 완료 뒤 rank 3 이동 계산에서 종료

실행 prefix는 `_local/artifacts/full4-symbolic-piece-cache-20261123`다. Progress log,
`.observation.json`, `.runtime.json`, `.summary.json`을 확인했다. 요청·source 지문과
release profile은 앞선 두 실행과 같으며, 시간 제한이나 memory/worker 설정을 바꿔
재시도한 실행이 아니다.

| 실행 식별자 | 값 |
| --- | --- |
| Compiled source commit | `1fe73ed88fa350ef0ebd9ae9e17281061fc2ff54` |
| Compiled source tree | `6bab3217ddcce8484f11685c3a1f96d4600cc809` |
| Binary SHA-256 | `965ee9045c36b5566e80929ccf070ed8658443598da5ec24c86469f5577c9aba` |
| Request fingerprint SHA-256 | `e709af430f47ba7292eba224b3af7414efaa13560082dc77c474089992afa924` |
| Source fingerprint SHA-256 | `989d0aa152b2a75913d02142fcca6df8d6e88302de116c84060fb22decc81d56` |

| 관측 | 확인된 값 | 해석 범위 |
| --- | --- | --- |
| Geometry compiler 완료 | Search 시작 뒤 0.928935797초 | Structured boundary의 실제 측정 |
| Compatibility certificate | 0.002136382초 | 합친 boundary는 0.931072179초 |
| Rank 2 완료 최초 관측 | Harness 시작 뒤 37.045454304초 | 이 sample에서 `symbolic_rank completed=2`, rank 3의 `board` 문맥 확인 |
| 프로세스 실행 시간 | 44.694초 | SIGKILL까지의 시간이며 search 완료 시간이 아님 |
| 마지막 계산 문맥 | `movement`, rank 3/10, piece·row 없음 | 마지막 sample은 harness 시작 뒤 44.058852350초 |
| 마지막 보고 compatibility memo | 0 entries / 6,112 bytes 추정 | Memo 해제 뒤 남은 `row_cells` 등의 capacity |
| 마지막 보고 joint ZDD | 786,432 nonterminal nodes | Backing capacity 추정 55,586,048 bytes |
| 마지막 Boolean work pulse | 224,898,786 | Rank 2의 앞선 관측 값은 47,435,175. 중첩 counter와 합쳐 총 연산 수로 해석하지 않음 |
| Process sampled RSS peak | 2,708,467,712 bytes | Sampler가 읽은 현재 RSS의 최댓값 |
| Process sampled HWM peak | 2,725,687,296 bytes | Sampler가 읽은 kernel HWM의 최댓값 |
| Manager sampled peak | 2,851,733,504 bytes | Manager 측정 범위의 값 |
| 종료·출력 | Signal 9, exit 137 | `search_complete=false`, CTK4 없음 |
| 시간 제한·재시도 | `timeout_seconds=0`, automatic retry 없음 | 종료 전 manager termination attempt 없음 |

Rank 2 완료 시점은 sampler의 최초 관측과 정확한 완료 clock을 구별한다.
36.044525097초 sample에서는 rank 2의 L/row 757과 마지막 완료 rank 1이 보였고,
다음 37.045454304초 sample에서 완료 rank 2와 rank 3 진입이 처음 보였다. 이 두
sample은 관측 경계를 제시하며, 37.045454304초를 rank 2의 정확한 완료 시간으로
기록하지 않는다. 진단 필드들은 독립적으로 읽으므로 한 sample 전체가 원자적인
실행 상태 snapshot인 것도 아니다.

종료 전 manager의 kill 시도는 없었다. 남은 `drop-cleanup` 시도는 child의 종료를
확인한 뒤 수행되었고 `ESRCH`로 실패했다. 실제 프로세스 종료와 이후 containment
정리를 같은 원인으로 취급하지 않는다.

이번에는 rank 2의 compatibility 계산을 마쳤고, 마지막 관측에서 해당 memo가
비어 있었다. 반면 rank 3의 board/movement 구간에서 Boolean work와 process RSS가
늘었다. 따라서 다음 조사 우선순위는 **Boolean 이동 관계 계산의 공유 prefix와
worker private suffix 및 그 중간 상태의 수명**이다. 현재 로그에는 movement의 free
predicate 생성, ordered-edge guard 생성, fixed-point 전파를 구분하는 worker별
계측이 부족하다. 전체 RSS 증가가 모두 worker private suffix에서 발생했다고
확정할 수 없으며, 공유 prefix·coordinator 저장소·allocator·일시적 apply/rehash
메모리도 구분해야 한다.

### 8.4 종료 상태와 Full6 조건

다음 항목은 결과를 추정해서 채우지 않는다. Source tree와 binary가 달라지는 후속
실행은 별도 prefix와 지문으로 구별하며, 이전 실패를 성공 baseline으로 쓰지 않는다.

| 실행 | Search 완료 | Search 시간 | Archive 결과 | 메모리·증거 상태 |
| --- | --- | --- | --- | --- |
| 첫 shipping Full4 P7P4 | 미완료 | 완료 시간 없음 | 없음 | 위 SIGKILL 관측과 지문 보존 |
| Prefix/semijoin 이후 Full4 P7P4 | 미완료 | 완료 시간 없음 | 없음 | 별도 SIGKILL 관측과 지문 보존 |
| 완료 piece cache 수명 변경 후 Full4 | 미완료 | 완료 시간 없음 | 없음 | Rank 2 완료, rank 3 movement에서 SIGKILL |
| 같은 조건의 비교 가능한 기준 경로 | 확인 대기 | — | — | 기존 미완료 실행을 완료 baseline으로 사용하지 않음 |
| Full6 | 실행하지 않음 | — | — | P7P4의 60초 미만 완료 조건 미충족 |

P7P4가 사용자 기준인 60초 미만에 완료되는지 확인한 뒤 Full6를 조건부로 검토한다.
이 60초는 다음 실험을 선택하는 기준이며 solver나 manager에 삽입하는 종료 제한이
아니다. Full6을 시도해도 결과 family의 폭, parallel private suffix, joint apply,
compaction·archive의 실제 메모리 병목을 별도로 관측해야 한다. 이번 구현과 유한
검증만으로 그 작업량이 이미 해결되었다고 주장하지 않는다.

## 9. 남은 구현·연구 작업

다음 목록은 후속 작업이며 이번 변경에서 구현하거나 성능을 확인한 내용이 아니다.
이번 실행으로 Full4 완료 결과를 얻지 못했으므로 full-result 동등성과 실제 성능도
아직 확인되지 않았다. 새 알고리즘 후보는 특정 큐 문자열이나 kick 이름을 위한
분기로 추가하지 않고 기존 exact source·line-clear·ordered-kick 의미를 유지해야 한다.

### 9.1 이미 구현된 전파를 재사용할 부분

`reachability_symbolic_family.rs::propagate`에는 이미 `reached`, `pending`, queue,
`delta`, `novel` 전파가 있다. Source에서 꺼낸 pending 집합을 delta로 삼아
`contribution = delta ∧ edge_guard`, `novel = contribution ∧ ¬reached[target]`를 구하고,
새 부분만 target의 reached와 pending에 합친다. 따라서 **delta 전파 도입 자체를
미구현 작업으로 분류하지 않는다**. Free predicate 생성도 이미 `self.domain`에서
시작한다.
[현재 free/edge 구성과 delta 전파](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_family.rs)

남은 연구 후보는 이 구조 안에서 반복되는 intermediate Boolean node, negation,
free/edge 구성, worker별 보유 상태를 줄이는 방법이다. 현재 `not`은 XOR와 TRUE의
apply로 구현되어 있다. Complemented edges는 이를 줄일 수 있는지 검토할 표현 후보며,
도입이나 개선 효과를 검증하지 않았다. Domain conditioning도 이미 있는 제약을
앞당기거나 재사용해 중간 표현을 줄일 수 있는지 검토하는 것이며, 지금 domain 조건이
없다는 뜻이 아니다.
[Boolean 표현과 apply](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_boolean.rs)

### 9.2 우선순위 체크리스트

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_family.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_parallel.rs`, `crates/clearra-core-executor/examples/symbolic_family_compare.rs`, 구현 내용: movement 안의 free predicate·ordered-edge guard·fixed-point·lock/row 조건 생성 경계를 worker별로 계측한다. 공유 prefix는 한 번만 계상하고 private suffix의 현재·최대 node와 capacity를 구분하며, callback에서 고빈도 formatting·I/O 없이 독립 sampler로 보고한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_family.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_boolean.rs`, 구현 내용: 기존 pending/delta/novel 전파를 재사용하면서 반복 negation, free/edge guard와 intermediate apply의 중복을 측정하고 재사용·융합 가능한 계산을 검토한다. 이미 적용하는 domain 조건과 ordered first-success kick 의미를 보존하며, conditioning의 위치 변경은 검증 전 연구 후보로 취급한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_boolean.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_bridge.rs`, 구현 내용: complemented-edge 표현의 적용 가능성을 검토한다. Canonical ID·terminal·cofactor·owner 범위·bridge 의미와 메모리 비용을 먼저 평가하고, 유한 정확성 대조 전에는 채택이나 속도 향상을 주장하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_parallel.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_boolean.rs`, 구현 내용: 공유 prefix와 worker suffix의 실제 사용 종료점을 확인하고, 완료 결과 소비 전후에 필요 없는 apply memo·임시 상태를 해제할 수 있는지 검토한다. 생성된 worker의 즉시 투입과 단일 joint owner를 유지하며, 현재 RSS 전체를 private worker 비용으로 가정하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/examples/symbolic_family_compare.rs`, `docs/research/symbolic-compatibility-ctk4-2026-11-23.md`, 구현 내용: 후속 후보가 준비되면 같은 source/request·resource 조건의 Full4를 별도 지문으로 계측하고 search·archive·검증 시간을 분리한다. 완료된 결과가 있을 때 identity/source 및 필요한 joint 관계 동등성을 검산하고, Full4가 60초 미만에 완료되기 전에는 Full6를 시작하지 않는다.

현재 소스는 검증한 최종 후보에서 동결했으며 이 목록에 따른 추가 알고리즘 편집이나
추가 탐색은 이번 작업에서 진행하지 않는다. CI와 알려진 100% 확률 명제에 대한
추가 검색·탐색·증명도 실행하지 않았다.

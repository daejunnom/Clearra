# 완성 후보 전개 없는 symbolic family 탐색: 구현 기록

작성일: 2026-11-23.

상태: `symbolic-family` feature로 명시적으로 선택하는 연구용 Rust 경로를 구현했다.
기본 제품 탐색기의 dispatch는 바꾸지 않았다. 새 기능 관련 유한 정확성 검사
38개와 native example·최소 feature WASM 컴파일 확인을 마쳤다.
이 문서는 현재 소스의 구조와 계약을 기록한다. Full 6L과 OP7P7의 대형 탐색은
이 구현으로 실행하지 않았고, 실행 시간·메모리 절감률·완주 가능성을 주장하지 않는다.
사용자가 확정 전제로 제공한 100% 확률을 재검색·재계산·재증명하지 않았다.

후속으로 사용자가 승인한 Full 4L / P7P4 비교를 수행했다.
[실행 기록](full4-symbolic-family-comparison-2026-11-23.md)에 동일 입력·바이너리,
두 SIGKILL 미완료 결과, geometry 생성자 차이와 rank 2 Down 병목을 정리했다.
아래 초기 구현 검증과 후속 대형 실행의 확인 범위를 구분해 읽어야 한다.

그 뒤 진행한 범용 호환성 필터·row-prefix 재사용·병렬 이동·CTK4 구현은
[후속 구현 및 계측](symbolic-compatibility-ctk4-2026-11-23.md)에 정리했다.
최종 후속 후보는 Full4 rank 2를 통과했지만 rank 3 이동 단계에서 SIGKILL로
종료됐으며, Full4 완주와 Full6 실행은 아직 확인하지 못했다.
빌드 중복·실제 Cargo timings·종료 발신자 조사 범위는
[빌드와 SIGKILL 진단](build-and-signal-diagnostics-2026-11-23.md)을 따른다.

설계 근거와 반례 검토는
[Full 6L: 완성 후보 전개를 제거하는 family 집합 전이](full6l-direct-family-evaluation-2026-11-23.md)에
있다. 그 문서는 구현 전 설계 기록이고, 여기서는 실제 구현과 남아 있는 경계를 구분한다.

## 1. 이번 구현의 범위

새 진입점은 `solve_symbolic_family(problem, limits, control)`이다. 결과는 해법
전체의 `Vec`가 아니라, 허용된 배치와 원래 큐 사이의 압축 관계를 소유한다.
완성된 기하 후보 하나를 생성한 뒤 Build로 검사하는 반복을 호출하지 않는다.
노드 한도나 표현 범위를 초과하면 명시적 오류를 반환하며, 후보별 검증기로
자동 전환하지 않는다.

이 변화는 모든 어려운 계산이 사라졌다는 뜻은 아니다. 현재 알고리즘은 각
배치 개수 단계에서 미노 종류별 이동 관계를 계산하고, 해당 종류의 catalog
row들을 순회해 집합 전이를 수행한다. 비용은 완성 후보 수에 직접 비례하는
반복에서, 살아 있는 관계의 폭과 Boolean/ZDD apply 및 geometry derivative의
크기로 이동한다. 이 표현이 Full 6L에서 작게 유지되는지는 아직 알 수 없다.

| 소스 | 구현 책임 |
| --- | --- |
| [symbolic_family.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family.rs) | 지원 범위 검사, rank별 전이, root 수명 관리, 결과와 계측 API |
| [geometry.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs) | 기존 exact-cover compiler에서 압축 geometry DAG를 직접 인계 |
| [geometry_partial_family.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_partial_family.rs) | 정확한 집합 ZDD, 투영·계수·페이지·compaction, 지연 Down 제약 |
| [symbolic_boolean.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_boolean.rs) | 넓은 변수 ID를 지원하는 ROBDD와 명시적 자원 거절 |
| [symbolic_family_bridge.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_bridge.rs) | ZDD 선택 집합과 Boolean 상태 조건 사이의 정확한 변환·필터 |
| [symbolic_family_supply.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_supply.rs) | compact source 관계, Hold·EOF를 포함하는 기존 공급 automaton 재사용 |
| [reachability_symbolic_family.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_family.rs) | 원본 점유·줄 삭제·보드 압축과 정확한 ordered-kick 이동 관계 |
| [examples/symbolic_family.rs](../../crates/clearra-core-executor/examples/symbolic_family.rs) | 명시적 native `compile`/`run`, JSON 계측 및 요청한 결과 페이지 |

공개 export와 모듈은
[Cargo.toml](../../crates/clearra-core-executor/Cargo.toml)의 `symbolic-family` feature로
제어한다. 이 feature 자체는 기존 CPU worker 수, worker 시작 시점, NUMA 배치,
Chromium 경로를 변경하지 않는다.

## 2. 상태와 전이

### 2.1 배치 선택과 원래 큐의 결합 관계

catalog의 row-ID는 보드의 수평 행 번호가 아니라 **미노 종류와 원본 footprint**를
식별한다. row-ID 우주를 R, 정확히 N개의 미노로 채워지는 완성 기하 family를
F라고 둔다. 검색 중 유지하는 것은 완성 cover C가 아니라 다음 관계다.

\[
L_k^a\subseteq\{(S,q):S\subseteq R,\ |S|=k\}.
\]

S는 지금까지 실제로 배치한 row-ID 집합, q는 원래 source의 큐 identity다.
a는 cursor, 현재 Hold와 terminal projection 소비 여부를 가진 작은 공급 상태로,
`BTreeMap<SymbolicSupplyState, ZddId>`의 key에 보관한다. ZDD 안에는 S와 q가
함께 들어간다. source 변수는 모든 row 변수보다 뒤에 배치한다.

각 전이는 다음 조건을 모두 만족해야 한다.

\[
L_{k+1}^{a'}=\bigcup_{a,r}\left\{
(S\cup\{r\},q)\;\middle|\;
\begin{aligned}
&(S,q)\in L_k^a,\ r\notin S,\\
&\operatorname{SupplyStep}(q,a,\operatorname{kind}(r),a'),\\
&\operatorname{Legal}_r(S),\\
&S\cup\{r\}\in\operatorname{Down}(F)
\end{aligned}
\right\}.
\]

같은 도달 상태를 만드는 다른 순서는 ZDD union으로 같은 원소에 합쳐진다.
이 계약은 `CountUnique`에 대응하며, 조작 순서나 스핀 trace의 개수를 세지 않는다.

### 2.2 완성 cover를 저장하지 않고 정확성을 유지하는 조건

\[
\operatorname{Down}(F)=\{S:\exists C\in F,\ S\subseteq C\}.
\]

각 단계에서 Down에 속한다는 것은 어떤 완성 cover로 확장할 수 있다는 뜻이다.
물리적 도달성은 Legal과 공급 전이가 별도로 보장한다. 단계마다 선택 수가 정확히
1씩 증가하고, 모든 C의 크기가 고정 N이므로 마지막에는
`S ⊆ C`와 `|S| = |C| = N`에서 `S = C`가 나온다.

반대로 합법적인 완성 cover C의 실행 순서가 존재하면 그 모든 prefix는 C의
부분집합이므로 Down에서 제거되지 않는다. 이 양방향 논증에는 같은 catalog,
고정 N, 정확한 이동 판정, source 상관관계 유지, 종단 공급 조건이 필요하다.
중간에 확장 가능한 witness C가 바뀌는 것은 문제가 아니다. S가 단조롭게 증가하고,
마지막 witness는 그 전체를 포함하기 때문이다.

### 2.3 Down 전체의 전개를 피하기

`compile_symbolic_geometry`는 기존 exact-cover compiler가 만드는
`Append / Union / Product` DAG를 직접 전달한다. 완성 후보 enumerator, 전체
노드의 밀집 `u128` path-count table, completion-oracle의 방문 표시는 만들지 않는다.
불변 DAG를 인계할 때 construction-only interner를 해제하며, 해제한 저장소의
크기를 `geometry_released_construction_bytes`로 기록한다. node와 ID는 유지한다.

`GeometryDownView`는 원래 DAG를 빌리고, 현재 필요한 row 선택에 대응하는
derivative와 joint ZDD의 교집합을 지연 평가한다. Down 전체를 먼저 canonical
ZDD로 바꾸지 않는다. 원본 footprint가 겹치는 서로 다른 row의 동시 선택은
catalog mask로 derivative의 앞 단계에서 제거할 수 있다.

Product를 사용할 근거는 기존 compiler의 셀 분리와 row 소유 불변조건이다.
**셀 분리는 물리·공급의 독립성을 뜻하지 않는다.** 지지, 이동 경로, kick의
선행 실패 판정, 전체 행 삭제, 공유 큐와 Hold는 전체 상태를 기준으로 다룬다.
Product의 자식별로 가능 여부를 구해 곱하는 방식으로 구현하지 않았다.

## 3. 점유·행 삭제·이동의 공유

### 3.1 원본 점유 U(S)와 현재 보드 B(S)

셀 c의 원본 점유는 초기 보드 I와 전체 catalog row 선택 변수로 정의한다.

\[
U_c(S)=I_c\lor\bigvee_{r:\,c\in\operatorname{cells}(r)}[r\in S].
\]

같은 셀을 포함하는 대안 row가 여러 개라도 모든 대안의 OR를 남긴다. 특정 완성
cover의 owner에 고정하지 않는다. U의 식은 request 전체에서 재사용하고, 각
rank의 현재 row-set domain에 제한하여 채워진 원본 행 D(S)와 삭제 후 B(S)를 구한다.

행의 잔존 순위를 추적하는 DP로 초기 셀과 선택된 미노를 같은 규칙에 따라 아래로
압축한다. 앞서 배치한 supporter가 행 삭제로 사라지는 경우도 B(S)에 반영된다.
정규화되지 않은 초기 완성 행은 owner 검사에서 거절한다.

높이 H는 최대 6이므로 삭제 행 frame의 공유 표는 최대 `2^H = 64`개다.
이것은 선택한 미노의 전체 `2^N` 부분집합을 후보별로 전개하는 처리가 아니다.
같은 rank의 모든 미노 종류와 row가 이 작은 frame 표를 공유한다.

### 3.2 모든 kick에 공통인 guarded movement

이동 pose, 수평·하강 이동, 회전, 상공 진입, 접지는 기존 `ReachabilityTemplate`이
소유하는 규칙을 재사용한다. SRS 전용 배치 조건이나 특정 미노만의 지름길을
추가하지 않는다. builtin NoKick, SRS, SRS+, SRS-X, Jstris 180과 연결된
imported ordered table을 같은 방식으로 다룬다.

회전의 j번째 kick은 그 이동 목적지가 비어 있다는 조건만으로 성립하지 않는다.
앞선 모든 kick이 충돌했다는 조건과 AND를 취한다. 앞선 후보가 충돌 없이
성립하면 이미 방문한 pose라도 뒤의 kick을 선택할 수 없다. nominal profile ID가
같은 imported table도 실제 순서가 다르면 같은 이동 owner로 다루지 않는다.

상공 seed에서 delta fixed-point를 구해 pose별 도달 family를 계산한다.
lock 판정은 실제 4셀 mask와 접지를 확인하며, 각 row의 ILC realization을
현재 삭제 frame에 연결한다. `Legal_r(S)`에는 r이 미선택이라는 조건도 들어간다.
같은 종류의 모든 row가 이 rank에서 한 번 만든 이동 관계를 사용한다.

여기의 도달성은 기존 compact PC의 상공 진입을 사용하는 Boolean lock 존재성이다.
조작열, spawn부터의 입력 시간, finesse, 스핀 종류의 증거를 새로 생성하는 계약은
아니다. 그런 요구는 현재 진입점에서 거절한다.

## 4. 큐·Hold·EOF와 상관관계

source 관계는 위치별 미노 종류를 나타내는 one-hot 변수로 원래 큐를 보존한다.
공급 상태 전이는 기존 canonical sequence automaton의 Current, SwapHeld,
StoreCurrent 규칙을 재사용한다. Hold의 finite terminal release와 필요한
lookahead projection 역시 기존 계약을 따른다. EOF 뒤에 임의의 다음 미노를
만들어 배치 가능성을 늘리지 않는다.

물리 domain을 만들 때는 `∃q,a: L_k^a(S,q)`만 Boolean으로 투영한다. 물리는
같은 S에서 큐와 무관하므로 이 공유가 가능하다. 그러나 실제 다음 단계로 전달할 때는
각 공급 상태의 **원래 joint relation**에 Legal을 적용한다. 서로 다른 큐에서만
성립하는 부분 결과를 섞지 않고, 이미 소비한 source 변수도 최종 coverage를 위해
보존한다.

ZDD에서 생략된 row 변수는 false라는 점도 중요하다. Boolean 변환 시 이를
don't-care로 취급하면 존재하지 않던 선택 집합을 추가하게 된다. bridge는
생략된 구간을 명시적인 false 조건으로 채운다. 반대 방향 필터 역시 ZDD에 없는
변수에서는 Boolean의 low 가지를 따라간다. source suffix는 필터 결과에 그대로
보존한다.

## 5. 현재 지원 범위와 명시적 거절

| 항목 | 현재 계약 |
| --- | --- |
| 기하 | 표준 폭 10, compact 높이 1–6의 Board64 PC, 양의 고정 `exact_pieces = N` |
| 문제 종류 | `OpeningPc` 또는 `ScenarioPc`, 목표 `ClearToEmpty` |
| 목적과 count | objective `All` 또는 `Unique`이면서 `PcCountPolicy::CountUnique` |
| 관측 | `FullQueueOracle` |
| source | 완전하고 균일한 compact factorized source, 지원되는 complete standard-bag source, 또는 실제 한 개의 고정 큐 |
| Hold | 공급 계약과 일치하는 Allowed/Forbidden, 일관된 초기 Hold, 시작 cursor와 bag epoch 0 |
| kick | 검증된 builtin 또는 해당 요청에 연결된 ordered-table authority |
| 실행 분할 | 전체 관계를 소유하는 단일 partition |

다음 요청은 의미를 근사하거나 조용히 무시하지 않고 `Unsupported`로 거절한다.

- CountAll, 점수, 실행 제약, Build replay evidence, Setup/Build query.
- colored identity 허용 목록, checkpoint schedule, 양수의 최소 잔여 큐 조건.
- 부분 관측/ObservedWindow, 불완전하거나 잘린 source, compact unique 큐 관계로
  연결되지 않은 다중 source, 지원하지 않는 초기 공급 상태.
- 고정 N이 없는 탐색, 현재 Board64 범위 밖, 여러 partition으로 분할한 호출.

owner 불일치, 잘못된 catalog/identity, 공급 길이·개수 불일치 등은
`InvalidProblem`이며, 메모리 할당 실패나 논리 한도 도달은 `Resource`다.
각 취소 owner는 `Cancelled`를 반환한다. 정상적으로 계산을 끝낸 공집합과 이
오류들을 구별한다. 지원 밖 요청에서 기존 탐색기로 자동 fallback하지 않는다.

## 6. 결과와 출력 경계

N단계 뒤에도 terminal 공급 조건을 통과한 관계만 accepted로 선택한다. accepted
relation E에서 세 가지 서로 다른 집합을 계산한다.

| 보고 값 | 의미 |
| --- | --- |
| `unique_solution_count` | source를 존재 양화한 고유 normalized placement identity 수 |
| `covered_source_count` | placement를 존재 양화한, 하나 이상의 해법이 있는 원래 큐 수 |
| `solution_source_pairs` | accepted relation의 서로 다른 `(identity, source)` 쌍 수 |

원본 geometry의 path-count나 Down의 derivation 개수를 이 값 대신 쓰지 않는다.
같은 `(piece, cells)` row 집합은 하나의 normalized identity이며, 서로 다른
실행 순서는 중복으로 합쳐진다. 같은 색만 남기는 Fumen identity와도 구별한다.
계수는 checked `u128`이며 overflow를 근사값이나 성공한 count로 바꾸지 않는다.

`identity_page(offset, limit)`는 계수된 DAG 가지를 건너뛰고 요청한 페이지만
materialize한다. offset 앞의 모든 identity를 출력하지 않는다.
`source_count_for_identity`는 retained joint relation을 제한해서 큐 수를 계산한다.
원래 source 목록을 다시 펼치지 않는다. 결과 전체 export에는 출력 크기에 비례하는
비용이 남으므로, 전체 export를 count의 필수 전처리로 만들지 않는다.

페이지 순서는 이 catalog/arena 안에서 안정적이지만 기존 normalized digest 순서와
같다고 보장하지 않는다. legacy `cts1` digest, 전 해법 정렬, 실행 trace와 제품 출력
어댑터는 이번 API가 제공하지 않는다. DAG hash를 기존 digest라고 바꾸어 부르지 않는다.

## 7. 메모리 수명과 계측의 한계

### 7.1 실제로 회수하는 것과 유지하는 것

각 rank 뒤에는 source root와 다음 공급 상태별 root만 지정하여 ZDD를 compact한다.
모든 살아 있는 root를 새 ID로 갱신하고 이전 ID를 가진 연산 memo를 버린다.
최종 결과도 accepted, identity projection, source projection 세 root를 함께
compact한다. source root를 빼거나 일부 root만 remap하면 큐 상관관계가 손상된다.

Boolean arena는 request 수명의 U(S)를 만든 직후 checkpoint를 잡는다. rank의
board와 movement relation을 모두 사용한 뒤 그 checkpoint로 rewind한다.
논리 노드와 apply memo를 줄이되 Vec/HashMap capacity는 다음 rank에서 재사용한다.
따라서 rewind를 RSS 반환으로 해석하지 않는다.

geometry derivative의 key는 원본 geometry owner에 속하며 ZDD root ID를 보관하지
않는다. ZDD compaction을 넘어 재사용할 수 있지만, 현재 orchestration에서는
derivative memo를 rank마다 비우지 않는다. `clear_derivatives` API는 있으며,
재계산 비용과 메모리 보유량을 판단해 사용할 여지가 있다. 원본 geometry의
construction interner는 불변 DAG 인계 시 해제하고 노드 저장소만 유지한다.

결과는 catalog와 최종 ZDD 관계를 소유한다. 원본 geometry DAG, derivative owner,
이동 predicate는 탐색 완료 뒤 결과에 포함하지 않는다.

### 7.2 추정 바이트와 RSS는 다른 계측이다

보고서는 geometry 노드, ZDD 생성·apply·compaction, rank별 live 노드, predicate
노드 peak, derivative 및 bridge 작업 수, 이동 pose·edge·전파 수를 구분한다.
`retained_*_bytes_estimate`와 compaction peak 추정은 capacity 기반 payload 회계다.
allocator metadata, HashMap의 실제 backing과 정렬, 모든 임시 stack/memo 및
동시 생존 객체를 포괄하는 엄밀한 상한이 아니다.

특히 compaction은 새 저장소를 만드는 동안 이전 저장소가 공존할 수 있다.
노드 개수가 줄었다는 사실만으로 순간 peak가 줄었다고 결론내리지 않는다.
서로 다른 시점의 retained 추정을 더해 프로세스 peak RSS라고 보고하지도 않는다.

native example은 Linux에서 `/proc/self/status`의 VmRSS와 VmHWM을 별도로 읽는다.
이는 해당 프로세스의 RSS와 high-water이며 자식 프로세스 합계가 아니다. 이번
기록에 새 대형 실행의 실제 RSS 측정값은 없다.

### 7.3 한도는 시간 제한이 아니다

`SymbolicFamilyLimits`는 geometry/set/predicate 노드, arena별 작업, memo 항목의
논리 한도를 받는다. 모두 0이면 그 한도는 무제한이다. 이 한도들은 프로세스 RAM
상한이나 탐색 완주 보증이 아니다. implicit deadline과 240초 제한을 넣지 않았다.
취소 토큰은 그대로 존중한다.

Boolean과 ZDD apply는 iterative 구조를 사용한다. Boolean의 trivial/cache-hit
경로는 scratch Vec를 만들기 전에 처리한다. memo 한도에 도달하면 무기록 재전개로
계산량을 키우지 않고 명시적인 `MemoLimit` 오류를 전달한다. 이 오류를 false나
빈 해법으로 바꾸지 않는다.

## 8. 병렬화와 성능 주장

현재 새 경로는 하나의 ZDD arena, Boolean arena, geometry derivative owner를
공유하는 **직렬 구현**이다. 미노 종류 및 row 순회도 직렬이다. native example의
전용 stack thread는 병렬 탐색 worker pool을 의미하지 않는다.

기존 제품의 worker를 생성되는 대로 투입하는 정책은 이번 변경으로 수정하지 않는다.
새 관계 연산을 병렬로 나누는 방법, thread 간 canonical ID 소유권, 중복 apply,
NUMA별 arena와 병합 비용은 별도 작업이다. 이번 구현을 NUMA 또는 Chromium
지연 최적화의 완료로 보고하지 않는다.

작은 입력 시간 측정이나 대형 탐색 벤치는 이 문서 작성에서 수행하지 않았다.
완성 후보 반복을 없앤 구조적 변경과 실제 처리량 향상은 구별한다. 최악의 경우
BDD 변수 순서, supply 상관관계, line-clear 조건, geometry derivative가 커질 수
있으며, 작은 원본 DAG가 작은 후속 관계를 보장하지 않는다.

## 9. 검증 상태

로컬 Rust manager의 `verification` profile과 `--no-timeout` 아래에서 순차 실행했다.
빌드의 debug 정보는 첫 실행부터 비활성화했다. OOM이나 자원 변경 후 재시도는 없었다.
CI 실행·조회·대기와 대형 탐색은 수행하지 않았다.

| 실행 | 확인한 결과 | 원시 기록 |
| --- | --- | --- |
| `cargo test --lib --features symbolic-family symbolic -- --test-threads=1` | 82개 중 80개 통과, 통합 fixture 2개 실패 | `_local/artifacts/symbolic-family-correctness-20261123.log` |
| 수정 후 `backend::wasm_cpu::symbolic_family::` | 통합 4/4 통과 | `_local/artifacts/symbolic-family-integration-20261123.log` |
| `geometry_partial_family` | 집합·Down·compaction 13/13 통과 | `_local/artifacts/symbolic-family-arena-tests-20261123.log` |
| `cargo check --example symbolic_family --features symbolic-family,parallel` | 통과 | `_local/artifacts/symbolic-family-native-check-20261123.log` |
| `cargo check --target wasm32-unknown-unknown --no-default-features --features symbolic-family` | 통과 | `_local/artifacts/symbolic-family-wasm-check-20261123.log` |
| 변경 Rust 파일 `rustfmt --check`, `git diff --check` | 통과 | source formatting 확인 |

처음 실패한 두 검사는 fixture의 전제를 수정했다. `0xfc3f0`의 2×4 빈칸은
OO로도 채울 수 있어 불가능 입력을 IO로 바꿨다. 또한 `[IO]1[IO]1`은 기존
compiler가 명시적인 다중 큐 저장소로 만들어 현재 symbolic admission에서
거절하는 입력이었다. 이 계약을 느슨하게 하지 않고, 실제 compact 원본을
유지하는 `[IO]1[IO]1P7`을 visible 2로 제한한 fixture로 교체했다.
보드 `0xfff0`의 서로 떨어진 I 빈칸에서는 원본 20,160개 가운데 II prefix를
가진 5,040개가 허용된다. Hold on/off 모두 기존 탐색 결과와 일치했다.

중복 실행을 제외하면 95개 테스트가 최종적으로 성공했다. 한 번의 실행에서
95개가 모두 통과했다는 뜻은 아니다. 이 중 신규 기능 관련 검사는 38개다:
Boolean 4, bridge 1, physical relation 5, supply 11, integration 4, ZDD/Down 13.
기존 symbolic 관련 검사 57개도 첫 실행에서 통과했다. 이 수치는 테스트
통과 개수이며 탐색 성능 측정이나 Full 6L의 축소 성능 추정으로 사용하지 않는다.

| 검증 파일 | 확인하려는 계약 |
| --- | --- |
| [symbolic_family_tests.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_tests.rs) | 기존 Unique 탐색기와 해법·source count 비교, 64개를 넘는 row 변수, 전 builtin의 줄 삭제, Hold/terminal 상관관계, 공집합·거절·취소 구별 |
| [reachability_symbolic_family_tests.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_family_tests.rs) | overlapping 대안 점유, 초기 셀과 6행 압축, 독립 raw 이동 oracle, 같은 tag의 다른 ordered table |
| [symbolic_family_supply_tests.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_supply_tests.rs) | compact queue 관계, 기존 automaton과 Hold/EOF 계약, 원래 source identity 보존 |
| [geometry_partial_family_tests.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_partial_family_tests.rs) | 집합 연산, Down derivative, 정확한 계수·페이지·compaction, 거절의 원자성 |
| [symbolic_boolean.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_boolean.rs) 및 [symbolic_family_bridge.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_bridge.rs) | 넓은 변수, iterative apply/rewind, memo 거절, missing=false, q 상관관계 |

유한 correctness fixture는 성능 측정이나 Full 6L 전체 탐색을 대신하지 않는다.
CI는 사용자의 별도 승인 이전에 실행하지 않는다.

## 10. 다음 판정 기준과 남은 작업

후속 개선은 완성 후보 수가 아니라 실제로 만든 상태와 관계의 크기를 확인해야 한다.
현재 코드에서 후보 enumerator와 Build를 호출하지 않는 것에 더해, 계측에서도
완성 후보 방문 0, 후보별 Build 호출 0, 전체 accepted identity materialization 0을
확인할 수 있어야 한다. 마지막 항목은 사용자가 명시적으로 요청한 page/export를
제외한 탐색·count 단계에 적용한다.

Full 6L을 곧바로 끝까지 실행하기 전에 rank별 노드 증가, apply miss, derivative
생성·재사용, movement propagation과 RSS high-water를 읽을 수 있어야 한다.
representation 한도에 의한 중단은 불완전한 실행으로 보고하며, 그것을 불가능 판정이나
성공한 무해법 탐색으로 제시하지 않는다. 큰 입력에서 실제 개선이 확인되기 전에는
제품 경로를 전환하지 않는다.

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family.rs`, 구현 내용: 후보/Build/전체 출력 비전개 계약을 계측 가능한 gate로 남기고 rank별 병목 판정에 필요한 항목을 보완한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_partial_family.rs`, 구현 내용: derivative의 rank 간 재사용과 해제 정책을 작업량·메모리 계측에 근거해 정하고 compaction의 순간 추가 메모리를 줄인다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_family.rs`, 구현 내용: 살아 있는 domain에서 실제로 반복되는 guard·edge·row 판정을 찾아 범용 공유 범위를 넓히고 모든 ordered kick의 의미를 보존한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family.rs`, 구현 내용: 미래 동작에 필요한 문맥과 q 상관관계를 보존하는 병렬 분할 및 NUMA owner 설계를 검토하고 기존 worker 즉시 투입 정책과 연결한다.
- [ ] 수정 파일: `crates/clearra-core-executor/examples/symbolic_family.rs`, 구현 내용: 승인된 후속 계측에서 bounded 작업과 과정별 RSS를 수집하고 Full 6L 또는 더 작은 대표 입력의 rank별 병목을 비교한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family.rs`, 구현 내용: 제품 통합 전 legacy digest·결과 페이지·coverage·취소와 Unsupported 전달 계약을 연결하고 지원하지 않는 CountAll/trace 요구를 계속 구별한다.

## 11. 사용 진입점

feature를 명시하면 라이브러리에서 다음 API를 사용할 수 있다.

```rust,ignore
let mut result = clearra_core_executor::solve_symbolic_family(
    &problem,
    clearra_core_executor::SymbolicFamilyLimits::default(),
    &control,
)?;
let exact_count = result.report().unique_solution_count;
let selected_page = result.identity_page(0, 10)?;
```

native example은 일반 height/board/queue/rule을 받는다. `compile`은 공급과
요청 계약만 준비하고 geometry 생성이나 symbolic 탐색을 호출하지 않는다.
아래는 저장소 루트에서 사용하는 compile-only 명령의 예시이며, 이번 검증에서
이 명령으로 Full 6L 탐색을 실행했다는 의미는 아니다.

```sh
build/cargo/default/debug/clearra-manage runtime run \
  --producer symbolic-family-compile --profile verification --no-timeout -- \
  cargo run -p clearra-core-executor --example symbolic_family \
  --features symbolic-family -- \
  compile --height 6 --board 0 --queue OP7P7 --hold --rule srs-plus
```

실제 탐색은 명시적인 `run`으로 구분한다. `--max-geometry-nodes`,
`--max-set-nodes`, `--max-predicate-nodes`, `--max-work`, `--max-memo-entries`는
선택적인 표현 한도이며 0은 무제한이다. `--page-size`의 기본값은 0이므로
해법 전체 출력이 계수 뒤에 자동으로 따라오지 않는다. 유한 한도나 취소 때문에
종료된 경우에는 완료한 전체 해법 집합을 반환하지 않는다.

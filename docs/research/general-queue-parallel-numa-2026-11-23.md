# 일반 큐/Hold 교집합 재사용과 native/Web 병렬 실행

## 1. 목적과 검증 상태

해당 목표의 확률 100%는 사용자가 제공한 확정 전제로 두며, 이 작업에서는 이를 추가로 검색·탐색·증명하지 않는다.

이 변경은 이미 존재하는 정확한 계산을 더 적은 반복 작업으로 수행하고, 최종
결과 조립과 worker 준비의 병렬성을 개선하는 작업이다. 큐/Hold 부분에서는
특정 Full6 inventory가 아니라 **같은 source 전이와 같은 후속 상태**의 동치를
사용한다. 기존 source/물리 검증, 전체 결과 열거와 CountUnique의 완료 기준은
유지한다. 구현 소스는 `e6dc9377e594015234abb903ff8347be95582631`로 고정했다.
Core 63개, Domain 14개, native affinity 3개, Node 계약 6개 검증을 통과했고,
P7P4 ABBA 네 실행에서 456,923개와 같은 canonical hash를 보존했다. 새 측정과
실행하지 못한 환경은 8–10절에 구분하며, 6절의 과거 측정은 새 구현의 성능
결과로 합산하지 않는다.

## 2. 정확한 물리 언어와 source/Hold product

`G`를 현재 요청의 규칙과 보드에서 만든 실제 BuildOrderGraph라 하고,
`L(G)`를 accepting graph path가 출력하는 종류 word의 언어라 하자. 하나의
종류 word를 만드는 구체적인 배치·회전·Hold 실행 이력은 여러 개일 수 있다.
Boolean coverage에 사용하는 kind projection과 CountAll의 구체 edge 경로는
서로 다른 consumer 계약이다.

원본 큐 ordinal의 집합을 `U`라 하면 source/Hold product의 논리 상태는 다음과
같이 쓸 수 있다.

```text
S = (graph node, source cursor, held piece, original ordinal bits).
```

실제 bitset 구현은 `(node, extra_draw, hold_code)`와 원본 ordinal word를
저장한다. cursor는 `initial_cursor + node.depth + extra_draw`에서 결정한다.
`extra_draw`는 empty Hold에서 StoreCurrent가 소비한 추가 draw를 보존하며,
현재 product의 상태에서 임의로 지우거나 다른 초기 Hold와 공유하지 않는다.

같은 graph edge가 물리적으로 실행 가능하다는 조건과, 해당 원본 큐/Hold에서
그 종류를 낼 수 있다는 조건은 **AND**다. 대안 physical realization, Hold
action, 후속 language edge는 각 consumer가 허용하는 범위에서 **OR** 또는
실행 이력의 합으로 처리한다. positive ILC 조건을 대안별 OR로 유지하고,
행 contributor의 필수 조건을 AND로 유지하는 기존 의미도 바꾸지 않는다.

정적 dependency DAG가 acyclic이라는 사실만으로 `L(G)`의 물리적 성공이나
어떤 큐와의 교집합을 단정하지 않는다. collision·grounding·SRS·spawn과
원본 source/Hold/EOF를 실제로 검증한 기존 graph/product가 권한을 가진다.
이번 재사용은 그 검증을 이미 통과한 동일 입력의 작은 계산만 공유한다.

### 적용하는 두 owner

| Owner | 실제 입력과 출력 계약 | 이번 재사용 |
| --- | --- | --- |
| [CoverageProductEvaluator](../../crates/clearra-core-executor/src/backend/wasm_cpu/coverage_product.rs) | `PatternPiecePositionIndex`가 표현하는 fixed/explicit/subset 등 materialized 원본 큐의 위치별 bits를 사용한다. BuildOrderGraph의 Boolean/CountAll consumer와 exact fixed-horizon kind-language adapter를 구분한다. | 같은-kind Current/Swap의 Boolean 후속 상태 중복 제거, Store의 같은 next-position mask 조회 재사용. |
| [StandardBagCoverage](../../crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage.rs) | 실제 Standard7Bag backing 또는 지원이 인증된 complete factorized layout의 source cursor를 symbolic decision DAG와 결합한다. atom 순서·choices·visible/full 길이·초기 Hold와 original ordinal 구조를 보존한다. | Current가 이미 검증한 첫 draw cursor를 Swap/Store에 재사용하고, 같은 desired kind의 Store 두 번째 draw를 language edges 밖에서 한 번 계산. |

이는 모든 source를 Standard7Bag로 치환하는 변경이 아니다. fixed/explicit
입력은 기존 bitset owner를 사용하고, 지원되는 symbolic source만 기존 symbolic
owner를 사용한다. partial atom과 materialized prefix의 실제 범위도 그대로다.
symbolic root가 더 넓은 full language를 표현하는 경우에도 `covers_any_pattern`,
coverage count와 최소 witness는 실제 materialized ordinal 범위와 교차한
기존 결과를 따른다. 원본 범위 밖의 accepting suffix를 요청 안의 성공으로
바꾸지 않는다.

## 3. Current와 Held가 같은 종류일 때의 Boolean idempotence

현재 original draw가 `c`, held piece가 `h`이고 같은 graph edge가 종류 `c`를
출력한다고 하자. `c=h`이면 Current와 SwapHeld는 다음 상태가 같다.

```text
Current:  (v, q, h=c, B_c) -> (v', q+1, h=c, B_c)
SwapHeld: (v, q, h=c, B_c) -> (v', q+1, h=c, B_c).
```

여기서 `B_c`는 현재 draw가 c인 실제 original ordinal bits다. source cursor,
held kind, child, extra-draw 상태와 원본 집합이 모두 같으므로 Boolean union은
`B_c OR B_c = B_c`다. 일반 bitset product에서 Current를 먼저 적용한 뒤
같은 상태를 만드는 Swap 활성화만 생략할 수 있다.

**CountAll에는 이 생략을 적용하지 않는다.** Current와 Swap은 같은 목적지를
가져도 서로 다른 labeled execution histories다. 이 consumer에서는 같은
상태로 들어가는 횟수를 `1+1`로 보존한다. graph의 여러 concrete pose edge도
Boolean 동치라는 이유로 합치지 않는다. 기존 symbolic Boolean product는
이미 같은-kind Swap을 생략하고 있었으며, 이번 변경은 일반 bitset Boolean
consumer에 같은 정확한 조건을 적용한다. 그 기존 변경은 `e9b899e6`에 있다.

대표 경로를 새로 선택하거나 action 우선순위를 바꾸지 않는다. Current가 이미
먼저 처리한 같은 후속 상태만 생략하고, witness ordinal과 실제 대표 Hold path는
기존 정확한 evidence 경로가 계속 계산한다. spawn guard가 필요한 다른 종류의
Swap은 기존 guard를 그대로 거친다.

## 4. Store의 bitset 공통 조건을 한 번 계산한다

현재 active original bits를 `A`, 현재 cursor에서 종류 k가 나오는 bits를
`C_k`, 다음 cursor에서 desired kind d가 나오는 bits를 `N_d`라 하자.
empty Hold의 StoreCurrent branch가 child에 전달하는 집합은 다음과 같다.

```text
Store(k, d): A AND N_d AND C_k.
```

같은 edge·source cursor·original word에서 `N_d`는 held로 남길 현재 종류 k와
무관하다. 따라서 `D=A AND N_d`를 한 번 구하고, 각 k branch에는 `D AND C_k`를
보낸다. AND의 결합법칙과 공통 조건의 분배를 사용하는 것이며, 서로 다른
held kind의 후속 상태를 하나로 합치는 것이 아니다.

`D=0`이면 모든 Store branch의 bits가 0이므로 그 edge의 Store 순회를 생략한다.
기존 `activate_transition`도 bits가 0일 때는 상태·queue·CountAll counter를
변경하지 않고 반환한다. 살아 있는 각 branch의 action 수와 순서는 그대로이므로
이 조회 재사용은 CountAll에서도 정확하다. mask 하나를 읽는 비용만 앞당기며,
큐 전체 prepass나 candidate별 큰 배열을 추가하지 않는다.

정상적인 source 위치, finite EOF, 알려진 projected piece는 계속
`piece_word_with_projected_standard_bag_lookahead`가 구분한다. terminal held
release의 기존 guard를 건너뛰거나 임의의 다음 피스를 만들지 않는다. 특히
Standard7Bag의 6-draw prefix에서 추론 가능한 유일한 일곱 번째 피스와,
factorized partial atom 뒤의 유한 source EOF는 같은 조건이 아니다.

## 5. Symbolic product의 첫 cursor advance 재사용

현재 source 상태를 s, 원본 current kind를 c, 다음 desired kind를 d라 하고,
이미 인증된 cursor table의 draw 전이를 `delta(s,c)`라 쓰자. Hold 행동은
source의 원본 draw 순서를 바꾸지 않으므로 다음 관계가 성립한다.

```text
s1 = delta(s, c)
Current:      next_source = s1,                 next_hold = old_hold
SwapHeld:     next_source = s1,                 next_hold = c
StoreCurrent: next_source = delta(s1, d),       next_hold = c.
```

기존 Current 계산은 같은 source와 c의 유효성 및 다음 cursor를 이미 확인한다.
따라서 Swap에서 다시 같은 첫 draw를 validate/advance할 필요가 없다. Store도
첫 draw를 반복하지 않고 두 번째 draw만 기존 cursor table로 검증한다. 같은
desired kind의 language edges가 여러 개면 그 두 번째 draw도 한 번만 계산한다.
각 edge의 서로 다른 language child와 이후 product/union은 계속 개별 처리한다.

이 권한은 동일 호출의 stack-local `current_step`에서만 나온다. 다른 candidate,
source owner, language arena, memo epoch에 cursor 결과를 새로 저장하지 않는다.
기존 product key의 `(language node, depth, exact remainder, hold)`와 source
owner namespace는 바뀌지 않는다. fixed queue나 다른 atom을 bag remainder가
우연히 같다는 이유로 합치는 규칙도 추가하지 않는다.

초기 Hold가 차 있거나 Hold가 금지된 경우의 branch admission, finite EOF의
held release, Standard6의 유일한 projected complement 조건은 그대로다.
projected complement를 사용하는 기존 경로에서도 이미 계산한 첫 draw만
재사용하고 두 번째 draw의 table 검증을 유지한다. Current → Swap → Store 및
같은-kind language edges의 union 순서도 보존한다.

## 6. 기존 메모리 절감과 성능 tradeoff

아래 값은 과거 고정 소스의 측정이다. 서로 다른 benchmark의 시간·peak를
합치거나 이번 구현의 성능 증거로 사용하지 않는다.

| 기존 변경 | 메모리 계약과 추가 비용 | 기존 근거 및 관측 |
| --- | --- | --- |
| Flat → adaptive state-major | row에 `(depth, remainder, hold)`를 한 번 저장하고 entry는 `language:u32 → root:u32`로 유지한다. directory와 migration scratch 비용, row 조회가 생긴다. 실제 directory 포함 payload가 flat보다 12.5% 이상 작을 때만 전환하며 작은 요청은 flat으로 시작한다. | [v081-selective-integration](v081-selective-integration-2026-09-27.md)의 같은 binary·11-worker P7P4 기록: 평균 121.009 → 117.548초, commit peak 6.466 → 5.830GiB, product+directory payload 37.577% 감소. 작은 P7은 92.705 → 93.972ms. 작은 표본과 pressure 차이 때문에 일반 속도 보장으로 확정하지 않았다. |
| Union compact 16B → 12B | 원래 u64 key를 두 u32로 보존하고 u32 value를 저장한다. 같은 hash·전체 key equality·SwissTable capacity/load 의미를 사용한다. key 재조립과 HashSet 접근은 필요하다. | [placement-dependency-prototype](placement-dependency-prototype-2026-11-20.md), `d9362685`의 단일 축 ABBA: 같은 슬롯의 union payload 1,792 → 1,344MiB로 25% 감소, sampled peak 중앙값 490.9MiB 감소. 시간 중앙값 8.06% 감소는 A 표본 사이 11.73초 변동 때문에 확정 가속으로 해석하지 않았다. `be8c57d9`에서 compact union을 제품 기본으로 반영했다. |
| Logical-live recycle | source product와 language cache의 합산 logical-live가 기존 256MiB 기준을 넘으면 exact union을 먼저 flush하고 epoch-local references와 memo를 비운다. capacity는 유지되며 같은 suffix를 나중에 재계산할 수 있다. retained/RSS hard cap이나 후보 생성 한도가 아니다. | [v081-accelerator-worker-product-closure](v081-accelerator-worker-product-closure-2026-09-27.md), [buildup.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs)의 `recycle_symbolic_caches_if_needed`, StandardBag의 `local_live_bytes`와 `flush_and_recycle_local_cache`. |
| Row retained 회계 | 이전의 전체 row capacity 순회를 mutation 시 실제 capacity delta 누적으로 대체했다. reserve 실패 뒤 실제 용량도 반영한다. | `ce6211c1`, [standard_bag_product_memo.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_product_memo.rs). 이 O(1) 개선은 이미 존재하므로 이번에 다시 구현하지 않는다. |
| Full-key equality와 optional admission | hash만으로 hit를 인정하지 않는다. memo allocation/admission 실패는 이미 계산한 exact root를 반환하고 저장만 생략한다. 같은 suffix의 재계산 비용은 생길 수 있다. | [compact_exact_u64_map.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/compact_exact_u64_map.rs)와 StandardBag memo insertion. `ec322ac3`의 고정 union entry limit은 test/local-search-ab 실험 범위이며 제품 기본은 `None`이다. |

이번 공급 product 재사용은 위 layout, promotion, immutable table 공유, entry
width, capacity, admission, recycle 기준을 바꾸지 않는다. 공급 product에 새
heap·memo 또는 source 전체 스캔도 추가하지 않는다. 실제로 제거한 반복 작업은
다음과 같으며, 호출 계수와 통합 wall time을 8–9절에서 별도로 보고한다.

| 반복 작업 | 기존 | 새 경로 |
| --- | --- | --- |
| Symbolic Swap의 첫 draw | Current 뒤에 같은 cursor advance 재실행 | 이미 검증한 Current의 다음 cursor 사용 |
| Symbolic Store | 같은 desired kind의 각 language edge에서 두 draw 계산 | 첫 draw 재사용 + desired kind당 두 번째 draw 한 번 |
| Bitset Store의 next-position mask | 현재 종류 최대 7개 각각에서 동일 조회 | edge/source word당 한 번 |
| Boolean Current=Held | 같은 successor와 원본 bits를 다시 활성화 | 기존 Current의 활성화를 유지하고 중복 Swap만 생략 |

테스트 전용 reference guard와 작업 계수는 제품 빌드에 들어가지 않는다.
`cursor advance 호출`, `position-word 조회`, `transition 호출/비어 있지 않은
활성화`를 비교하며, 실제 product states·edges·original bitmap과 메모리 payload는
별도로 비교한다. 방문량 감소만으로 전체 실행 시간 개선을 주장하지 않는다.

## 7. 최종 구현과 worker 실행 범위

### 7.1. 최종 결과 조립

[terminal_parallel.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/terminal_parallel.rs)는
이미 모인 identity의 canonical 정렬과 문자열 생성을 대상으로 한다. exact Ord와
Eq, caller의 dedup, 기존 `cts1` 직렬화 계약을 유지한다. identity slice를 복사해
정렬하지 않고 기존 allocation의 disjoint slices를 사용하며, 문자열은 기존
결과 slots의 순서대로 만든다. 작은 입력과 WebAssembly는 직렬 경로다.

정렬은 참가자당 최소 65,536개, key 생성은 최소 8,192개를 기준으로
`useful_workers = max(1, min(requested, items / minimum))`을 계산한다. 이
budget은 **호출자를 포함한 해당 단계의 동시 작업 참가자 수**다. 유휴
persistent pool까지 포함한 프로세스 전체 OS thread 수를 뜻하지 않는다.
이미 정렬됐거나 역순인 큰 입력은 먼저 확인해 불필요한 분할을 피한다.
명시적 memory-bound와 parent-authorized 결과 조립은 기존 직렬 admission을
유지한다. 작은 입력에는 scoped thread나 그 메모리 예약이 추가되지 않는다.

새 hash 함수는 이미 만든 canonical 문자열의 bytes와 NUL delimiter를 읽어
동일한 `cts1`을 계산한다. 외부 임의 문자열을 검증하는 API는 아니다. 정상
complete parallel 결과는 incoming identity Vec를 직접 정렬·dedup해 최종
결과로 이동하며, 이전 mutable hash set에 다시 넣는 왕복을 생략한다. 다른
evidence·score·partial 결과 경로의 기존 계약은 보존한다.

### 7.2. Frozen family의 독립 준비 단계

[geometry_family_preparation.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_family_preparation.rs)는
같은 immutable family를 읽는 path-count와 row-order table 준비를 caller와
scoped worker 한 개에 나눠 겹친다. Native parallel, physical-prefix 경로,
요청 참가자 2명 이상, family node 262,144개 이상에서만 선택하며, 명시 family
retained cap과 resource-authoritative compiler는 기존 직렬 경로를 유지한다.

`freeze_compiler`의 소비 경계에서 construction index뿐 아니라 residual/domain/
projection 등 compiler scratch도 먼저 drop한다. 그 뒤 기존 두 결과 Vec를
만들므로 compiler scratch를 더 오래 붙잡거나 새로운 O(N) 배열을 복제하지
않는다. 기존 직렬 구현에서도 최종 두 table은 공존했다. cancellation은 join
후 실제 `Cancelled`로 전달하고, path-count overflow와 optional row-table
unavailable의 `None` 의미를 구분한다. 메모리 예약 또는 spawn이 불가능하면
같은 직렬 recurrence로 계산한다.

일반 작은 family는 `SOURCE=false` const specialization을 사용한다. 실제
full-15 source가 인증된 경우에만 `SOURCE=true` 경로를 선택하므로, 작은
요청의 매 Append마다 Full6 전용 Option 검사와 bookkeeping을 반복하지 않는다.
이는 기존 source 필요조건의 활성 범위를 넓히거나 후보를 새로 배제하는 변경이
아니다.

### 7.3. Scoped thread의 추가 메모리와 compute 권한

[scoped_thread_memory.rs](../../crates/clearra-core-executor/src/resource/scoped_thread_memory.rs)는
추가 background thread당 **256 KiB stack + 64 KiB metadata allowance**를
기존 shared memory authority에 예약한다. 새 `ResourceLeaseRequest::memory_only`
생성자는 compute 0, 양수 memory만 허용하며, 기존 `new(0, ...) == None`은
유지한다. 기존 검색의 compute 참가자 budget을 다시 획득하거나 변경하지 않는다.

Terminal의 유효 참가자 수가 W이면 신규 thread는 최대 W−1개이며, 준비 overlap은
1개다. lease는 모든 join까지 유지된다. 정렬과 key 생성은 순차 phase이므로
두 phase의 stack 예산을 동시에 사용한다고 더하지 않는다. 여유 메모리 예약이
불가능하면 결과 조립만 기존 직렬 계산으로 돌아간다. 이 예산은 추가 scratch의
논리 예약이며 OS RSS, allocator arena, idle persistent pool stack의 정확한
측정치가 아니다. Ordinary exact admission의 기존 한 bitset projection을
전체 실행의 heap cap으로 오인해 검사하거나, 이번 lease를 전체 payload의
재인증으로 사용하지 않는다.

### 7.4. Native NUMA와 Web 준비

Native persistent pool은 이미 준비된 worker와 caller가 나중 worker의 생성
완료를 기다리지 않고 일을 받도록 한다. Topology는 실제 허용 CPU 집합과
완전히 식별된 여러 NUMA node의 교집합에만 한정한다. 각 worker는 시작할 때
최신 inherited affinity와 다시 교차하며, main thread는 pin하지 않는다.
Scoped terminal worker는 자기 자식 생성 뒤 leaf 작업 직전에 locality를 적용해
자식의 허용 집합을 먼저 좁히지 않는다. 단일/불명확 topology, API 실패,
Windows multi-group 등 지원 밖의 경우에는 기존 OS scheduler를 유지한다.
worker 수 축소나 catalog/workspace의 NUMA별 복제는 없다.

Web의 [ClearraVerifierPool](../../apps/clearra-web/src/workers/ClearraVerifierPool.ts)은
client 생성 직후 initialize/prewarm을 시작하고, 누적 준비 작업이 8ms를 넘을
때 host에 yield한다. 준비된 worker는 나머지 worker와 독립적으로 task를 받는다.
명시적 ready-subset 종료, generation 교체, cancel/failure는 아직 생성하지 않은
sibling의 시작을 막는다. 일반 finish는 기존 전체 초기화 계약을 유지한다.
초기화 실패는 host yield 중에도 전파되며, lazy delegation은 같은 요청에서
한 번 만든 Promise/권한을 공유한다. 원본 task의 완료·전송·권한 계약은 유지한다.

## 8. 실행 검증과 실제 작업 계수

실행 자료는 `_local/artifacts/general-queue-parallel-20261123/`에 보존한다.
`runtime-summary.json`은 각 로그의 matching receipt를 수집한 요약이고,
`measurement-summary.json`은 Rust helper가 계산한 측정 요약이다. 이 문서에는
선별한 결과만 기록하며 raw stdout/stderr는 넣지 않는다.

| 검증 | 최종 결과 | 근거 |
| --- | --- | --- |
| Core focused | 63 PASS, 0 FAIL, 2 ignored; 37.76초 | `final-focused-tests.log`, receipt `1795375614743845393-2` |
| Domain | 14 PASS; 0.00초 | `domain-tests.log`, receipt `1795375673309285527-2` |
| Native affinity FFI | 3 PASS; 0.00초, 별도 컴파일 9.69초 | `affinity-tests.log`, receipt `1795374844363523747-2` |
| Node 계약 | 6개 실행 통과 | `web-worker-contracts.log`; 실제 Worker/browser 성능과 구분 |
| WASM32 compile check | PASS; 15.10초, dead-code warnings 7개 | `wasm-check.log`, receipt `1795376226474200779-2` |
| 최종 synthetic terminal ABBA | 1 PASS; 4.62초 | `terminal-assembly-final-abba.log`, receipt `1795376466142129585-2` |

Core 일반 실행의 ignored 2개는 큰 source-prefix cache 진단과 terminal synthetic
ABBA다. 후자는 위와 같이 별도 최종 진단으로 실행했다. 큰 source-prefix cache
진단은 이번 작업에서 실행하지 않았다. Domain 로그의 synthetic panic 두 개는
기존 unwind 회귀가 의도적으로 발생시킨 것이며 해당 테스트도 PASS다.

| 검증 파일 | 실제 검증한 계약 | 상태 |
| --- | --- | --- |
| [StandardBag supply_reuse_tests.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage/supply_reuse_tests.rs) | StandardBag prefix·초기 Hold·EOF·factorized partial atoms. Flat/StateMajor 각각 root·bitmap·최소 witness·states/edges·retained/local-live·memo capacity/payload·recycle 뒤 union 동치. | 2 PASS |
| [coverage_product_supply_reuse_tests.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/coverage_product_supply_reuse_tests.rs) | Fixed/explicit, sparse 130-original subset와 zero-weight ordinal, 초기 cursor/Hold, partial atom, finite EOF/Standard6. 독립 canonical finite-Hold oracle의 original bits와 CountAll labeled histories, retained bytes 동치. | 3 PASS |
| [terminal_parallel_tests.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/terminal_parallel_tests.rs) | Small/cfg gate, worker budget, 정렬·dedup의 pointer/capacity, placement 0..16의 key/hash. 524,545개 identity를 1/2/8 참가자로 정렬하고 65,793개 prefix key/hash, sorted/reverse도 비교. | 일반 4 PASS, 별도 synthetic 1 PASS |
| [geometry_family_preparation_tests.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_family_preparation_tests.rs) | 기존 table recurrence·candidate 순서, cancellation/join, overflow/optional fallback, compiler scratch 해제, 실제 262,144-node product dispatch. | 6 PASS |
| [shared_resource_lease.rs](../../crates/clearra-core-domain/src/resource/shared_resource_lease.rs) | Memory-only가 compute를 소비하지 않음, 동일 authority의 cap/contention, child grant와 deferred parent drop, 0/u64::MAX/epoch 경계. | 새 3개 포함 Domain 전체 PASS |

초기 Standard6 회귀 실패는 기존·개선 product 불일치나 동시 실행 상태 오염이
아니라 visible 6개·empty Hold의 6-lock projection을 finite 7-draw EOF 계약으로
모델링한 fixture 오류였으며, 6개·empty Hold/6 locks, 6개·occupied Hold/7 locks,
실제 finite 7개/7 locks를 분리한 뒤 독립 oracle와 기존·개선 결과가 모두 일치했다.

아래 감소율은 `(old−new)/old`의 정확한 분수에서 계산했다. 서로 다른 fixture의
호출량이며 전체 검색 시간 감소율이나 물리 방문량 감소율로 해석하지 않는다.

| 계수 | Old → new | 정확한 감소 분수 | 감소율 |
| --- | ---: | ---: | ---: |
| Symbolic StandardBag cursor advance | 29,640 → 16,936 | 1,588/3,705 | 42.8610% |
| Symbolic factorized cursor advance | 24,476 → 15,944 | 2,133/6,119 | 34.8586% |
| Sparse Boolean position reads / transition calls 각각 | 18 → 16 | 1/9 | 11.1111% |
| Sparse Boolean nonempty activations | 8 → 6 | 1/4 | 25.0000% |
| Standard6 empty Hold position reads | 1,795 → 716 | 1,079/1,795 | 60.1114% |
| Standard6 empty Hold transition calls | 1,135 → 632 | 503/1,135 | 44.3172% |
| Standard6 initial L position reads | 104 → 103 | 1/104 | 0.9615% |
| Standard6 initial L transition calls | 105 → 104 | 1/105 | 0.9524% |
| Standard6 initial L nonempty activations | 20 → 19 | 1/20 | 5.0000% |

Sparse CountAll은 position reads/transition calls/nonempty activations가 각각
18/18/8로 그대로다. Standard6 empty Hold의 nonempty activations도 190으로
같다. Fixed/partial-atom fixture는 position-word 조회 66회 감소를 기록했으나
로그에 old 총량이 없어 백분율은 보고하지 않는다. 위 두 공급 owner의 메모리
동일성은 retained/local-live와 실제 capacity/payload assertion으로 검증했으며,
전체 프로세스 RSS가 동일하다는 주장으로 확장하지 않는다.

## 9. 고정 소스의 성능 측정

### 9.1. 소스와 실행파일

검증한 로컬 코드 `e6dc9377e594015234abb903ff8347be95582631`와
[GitHub 코드 commit](https://github.com/daejunnom/Clearra/commit/dd36cad460a2f38dc72980de469ac2905e994d11)의
tree는 모두 `9cdf0cee11bf55463fdf84801f09cd6324f9f048`이다. GitHub에는 이 코드
commit 다음에 본 검증 문서를 추가하며 기존 작업 branch를 fast-forward한다.
Old 실행파일 source에서 작업 시작점 `d67f3fa1`까지의 차이는 문서 commit뿐이다.

측정 환경의 허용 CPU는 0–8(논리 CPU 9개), `Mems_allowed_list`는 0이었고
NUMA sysfs topology는 노출되지 않았다. 따라서 이 환경의 native NUMA 경로는
fallback 상태에서 계측한 것이며, 다중 NUMA node에 실제로 배치한 성능 측정은
아니다. P7P4 integration의 실제 참가자 8명은 이와 구분되는 기존 요청 정책이다.

| 역할 | 고정 source | Binary SHA-256 |
| --- | --- | --- |
| P7P4 및 작은 입력 old | `e57962cf0e553d9fde7ab700fe4def2a19f70261` | `c00dcc36b7f3584e5497d687e3c0fb88d08af1d512e3d6e7e87e8f5010870132` |
| P7P4 및 작은 입력 new | `e6dc9377e594015234abb903ff8347be95582631` | `d4913e748d33985ac79028cc31aafb77b3dbf4b25663ba331feb681b9cf6fc51` |
| 최종 terminal 조립 진단 | 같은 new source의 실제 모듈을 포함하는 local Rust harness | `aaf7be5b9526b2dde1769294968f6c1b46881e362b2027d1c130730c94e79bae` |

Old 실행파일은 `_local/artifacts/full6l-p1p7p7-20261122/p7p4_exact-e57962cf`,
new는 이번 artifact root의 `p7p4_exact-e6dc9377`이다. 최종 조립 진단은
`_local/tmp/terminal_assembly_bench.rs`와 `terminal_assembly_bench-final`이며,
실제 terminal/worker/resource 모듈과 memory-only stack lease를 포함한다.
이전 `terminal-assembly-abba.log`는 lease 연결 전 중간 자료이므로 아래 성능
수치에 사용하지 않는다.

New integration test의 release build와 WASM check 명령은 다음과 같다.
실행 benchmark는 Rust manager의 `benchmark-search` profile과 각 receipt의
timeout으로 감독하며, 명령을 재실행할 때도 해당 감독을 유지한다.

```sh
cargo test --locked --offline --release -p clearra-core-executor \
  --features parallel,wasm-stage-profiling \
  --test wasm_cpu_parallel_exact --no-run

clearra-manage runtime run --producer general-queue-p7p4-reproduce \
  --profile benchmark-search --timeout 180 -- \
  _local/artifacts/general-queue-parallel-20261123/p7p4_exact-e6dc9377 \
  p7p4_parallel_matches_exact_solution_oracle \
  --exact --ignored --nocapture --test-threads=1

cargo check --locked --offline -p clearra-core-executor \
  --features parallel,wasm-stage-profiling \
  --target wasm32-unknown-unknown --lib
```

`local-search-ab` feature는 사용하지 않았다. 소스 변경 없이 보존한 두 실행파일을
ABBA 순서로 실행했으며, P7P4 query는 기존 Opening 4L Standard7Bag·empty Hold·
Unique 계약과 warmup을 사용한다. 각 실행은 count/hash뿐 아니라 count complete,
정상 parallel family 완료와 active worker 8도 기존 integration assertion으로
검사한다.

### 9.2. P7P4 전체 열거 ABBA

| Arm | Test 시간(초) | Sampled process-group RSS peak(bytes) | GiB | Receipt |
| --- | ---: | ---: | ---: | --- |
| A1 old | 72.41 | 4,174,082,048 | 3.8874 | `1795375118893805687-2` |
| B1 new | 69.62 | 4,164,485,120 | 3.8785 | `1795375845784625643-2` |
| B2 new | 71.37 | 4,093,558,784 | 3.8124 | `1795375939394580898-2` |
| A2 old | 75.22 | 4,221,562,880 | 3.9316 | `1795376022325689169-2` |

네 실행 모두 **456,923개**, **`cts1:98ebe8726537b29f`**, **active 8**이다.
각 arm 두 표본의 중앙값은 old 73.815초, new 70.495초로 관측 감소는
**3.320초 / 4.4977%**다. 이는 여러 변경을 함께 적용한 고정 입력의 ABBA
관측이며 개별 최적화의 단독 기여나 일반적 가속 보장이 아니다.

Peak는 kernel high-water mark가 아니라 matching receipt의
`linux-sampled-process-group-rss` 값이다. 네 실행 모두 normal/exit 0,
pressure event 0, automatic retry false다. Supervisor duration은 test harness
시간과 별도이며 각각 72.503/69.708/71.448/75.317초였다.

### 9.3. 작은 입력의 새 프로세스 cold/ABBA

`_local/tmp/compare_existing_tests.rs`는 위 old/new binary의 기존 integration
test 두 개를 `--exact --nocapture --test-threads=1`로 실행한다. Case마다 arm별
첫 호출 1회와 ABBA 4 round를 모두 보존했으며 **36 child process 모두 PASS**다.
각 시간은 `Command::output` 주위의 Instant로 잰 process startup + 전체 기존
test + stdout/stderr pipe drain/wait이며 search-only 시간이 아니다. Cold는
runner에서의 첫 호출을 뜻하고, 파일/OS cache를 비우지 않았다. ABBA도 매
표본이 새 프로세스이므로 같은 worker pool을 재사용하는 warm search 측정과
다르다. 근거는 `small-abba.log`, receipt `1795376176644928762-2`다.

| 기존 case | Cold old/new(ms, 각 1회) | ABBA old 중앙값(ms) | ABBA new 중앙값(ms) | Old 범위(ms) | New 범위(ms) |
| --- | --- | ---: | ---: | --- | --- |
| 1미노, unsplittable family의 직렬 gate | 9.275599 / 4.156456 | 2.330008 | 2.308593 | 1.952288–4.744634 | 1.621158–5.046949 |
| 5미노, work-based parallel과 canonical parity | 67.818833 / 65.852120 | 60.630986 | 61.703478 | 57.937491–69.255270 | 56.419354–66.820399 |

ABBA 중앙값은 각 arm 8개 표본에서 계산했다. 5미노는 new 중앙값이 약
**1.77% 높으며** 양쪽 범위가 겹친다. 이 표본으로 작은 입력의 지연 증가가
없다고 확정하지 않는다. 모든 표본과 cold 결과를 남겼고 빠른 값만 고르지 않았다.

### 9.4. 최종 synthetic terminal 조립 ABBA

Codec-valid 15-placement synthetic identities 456,923개를 대상으로 한다.
실제 물리 성공 후보나 검색 입력은 아니다. A는 기존 serial sort→identity
hash→keys, B는 새 sort→keys→serialized-string hash이며, fixture 생성과 입력
복사는 timed 구간 밖이다. 최종 memory lease 연결을 포함한 진단만 보고한다.

| Arm | Sort(µs) | Keys(µs) | Hash(µs) | 전체(µs) | Sort/key 참가자 |
| --- | ---: | ---: | ---: | ---: | --- |
| A1 | 328,250 | 872,441 | 170,964 | 1,371,656 | 1 / 1 |
| B1 | 109,988 | 216,464 | 179,067 | 505,521 | 6 / 8 |
| B2 | 103,211 | 157,485 | 171,749 | 432,447 | 6 / 8 |
| A2 | 339,221 | 610,203 | 174,831 | 1,124,256 | 1 / 1 |

전체 중앙값은 **1.247956초 → 0.468984초**, 관측 감소는 **62.4198%**다.
B의 요청 budget은 8이며 identity 수와 최소 분할 크기 때문에 sort 유효 참가자는
6이다. 네 arm의 전체 identity/key/hash 비교가 통과했고, identity capacity
456,940, key capacity 456,923, 합산 String capacity 148,499,975 bytes,
hash `cts1:e0646ed2d2df3082`가 같다. 추가 stack은 7.3절의 별도 예산이다.
이 진단은 terminal 조립에 한정되며 전체 검색이 62% 빨라졌다는 의미가 아니다.

## 10. 검증 한계와 남은 직렬 의존성

실제 Chromium 실행은 page/worker가 실행되기 전에 process-singleton socket의
`EPERM`으로 차단됐다(`chromium-orchestration.log`, receipt
`1795374984152712980-2`). 따라서 Node 계약 PASS와 WASM compile PASS를 실제
브라우저에서의 worker 속도·응답성 검증으로 대체하지 않는다. TypeScript 전체
`tsc`는 실행하지 않았고, Windows target compile/실제 Windows 실행과 실제
다중 NUMA 머신에서의 성능 측정도 하지 않았다. Native affinity 단위 회귀는
허용 집합 교차·fallback 의미를 검증하지만 NUMA의 실측 가속을 증명하지 않는다.

FamilyCompiler의 residual DAG 생성은 아직 의존 순서에 따라 진행한다. 이번에
겹친 것은 **완성된 family의 서로 독립적인 두 읽기 pass**다. 개별 candidate의
물리 build-order 그래프와 source/Hold product 안에도 실제 predecessor 및 memo
의존성이 남으며, 서로 다른 후보를 worker에 분배하는 기존 경계를 유지한다.
최종 dedup·일부 coverage/reduction·대표 경로 선택·canonical hash는 여전히
순서를 지켜야 한다. 이 단계들을 실행 수가 적다는 이유로 생략하거나 미완료
열거를 완료로 표시하지 않는다.

현재 자료는 일반 큐/Hold의 동치 재사용, 준비·조립의 독립 병렬화와 P7P4의
완전한 결과 보존을 입증하는 범위다. 이번 측정에는 새 Full6 전체 열거 완주
기록이 없으며, Full6 완료나 미검증 환경의 성능을 이 결과로 주장하지 않는다.

새 Rust 코드는 [Google Rust Style Guide](https://google.github.io/styleguide/rustguide.html)를
참고해 borrowed disjoint slices, 명시적인 전략 enum, unsafe FFI의 경계 분리와
rustfmt를 적용했다. Chromium 코드베이스에 한정된 `[chrome]` 지침을 standalone
Cargo 프로젝트 전체의 표준 thread 사용 금지로 확대하지 않았다.

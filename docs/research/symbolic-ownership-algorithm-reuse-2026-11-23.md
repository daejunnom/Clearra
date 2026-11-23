# Symbolic 탐색의 Boolean 표현과 생성용 저장소 수명

작성일: 2026-11-23.

상태: complemented-edge Boolean 표현, 완료된 worker의 생성용 인덱스 해제,
기존 kick template 증명 재사용, domain/source 조건의 반복 결합 제거,
전역 Boolean 간선 guard를 생성하지 않는 delta 전파를 구현했다.
**54개 선택 로컬 검증은 통과했지만, 세 번의 Full4 P7P4 실측은 모두 SIGKILL로
끝나 미완료다.** 최종 후보는 rank 3의 일곱 미노 모두 실제 도달 집합 전파에
있었다. 전체 실행 시간이나 전체 peak RSS 개선은 입증하지 못했고, rank 2 완료
최초 관측도 최종 후보가 더 늦었다. Full6 진입 조건을 충족하지 않아 실행하지 않았다.

이 작업은 worker 수나 사용자가 정한 자원 한도를 낮추지 않는다. 공통으로 필요한
계산과 저장소가 어떤 owner에 귀속되고, 언제부터 읽기만 필요한지를 기준으로
중복을 줄인다. 기존 전체 논리 worker 정책과 준비된 worker의 즉시 시작을 유지한다.
알려진 100% 확률 명제에 대한 추가 검색·탐색·증명과 CI 실행은 하지 않는다.

이전 구현과 실행 범위는 [호환 필터·병렬 이동·CTK4 기록](symbolic-compatibility-ctk4-2026-11-23.md),
외부 종료 원인의 확인 범위는 [빌드와 SIGKILL 진단](build-and-signal-diagnostics-2026-11-23.md),
family 전이의 기본 계약은 [초기 symbolic 구현 기록](direct-symbolic-family-implementation-2026-11-23.md)을
참조한다. 이 문서는 그 기록 이후의 Boolean 변경을 구분한다.

## 1. 이전 실행에서 확인한 단계와 메모리

관측 자료는 `_local/artifacts/full4-symbolic-piece-cache-20261123.observation.json`이다.
그 자료에 기록된 실행 바이너리의 source commit은
`1fe73ed88fa350ef0ebd9ae9e17281061fc2ff54`, source tree는
`6bab3217ddcce8484f11685c3a1f96d4600cc809`다. 이후 문서 commit이나 현재
작업트리를 이미 실행한 바이너리의 identity로 바꾸어 적지 않는다.

해당 실행은 rank 2 완료를 37.045454304초 표본에서 처음 관측했고, rank 3
이동 단계에서 44.694초의 manager duration으로 끝났다. 반환값은 137이고
SIGKILL을 관측했다. 아래 두 시점 사이에는 다른 arena의 누적 work 값이
고정된 상태에서 Boolean work와 child RSS가 증가했다.

| 관측값 | 40.051126471초 | 44.058852350초 |
| --- | ---: | ---: |
| 단계 | rank 3, movement | rank 3, movement |
| Boolean work | 113,749,730 | 224,898,786 |
| Bridge work | 53,739,520 | 53,739,520 |
| Compatibility work | 329,252,864 | 329,252,864 |
| ZDD work | 409,206,784 | 409,206,784 |
| Compatibility memo 항목 | 0 | 0 |
| Compatibility 잔존 추정치, bytes | 6,112 | 6,112 |
| 마지막 ZDD storage pulse의 추정치, bytes | 55,586,048 | 55,586,048 |
| Child RSS, bytes | 1,356,570,624 | 2,708,467,712 |
| Child VmHWM, bytes | 1,453,613,056 | 2,725,687,296 |

Compatibility의 0은 completed-piece 해제 때 보내는 명시적 marker다. 반면
ZDD 저장소 값은 마지막 저빈도 pulse이며, 새 pulse가 없으면 compaction 이후의
현재 저장소와 다를 수 있다. 이 값을 RSS에서 빼서 Boolean 저장소의 정확한
크기로 간주하지 않는다. Child RSS와 manager의 process-group 표본은 범위와
관측 시점도 다르다. Manager가 관측한 최대값은 2,851,733,504 bytes였다.

이 기록은 **이동 계산 중 Boolean 작업과 메모리가 함께 증가했다는 근거**다.
어느 worker의 nodes, unique table, apply memo, 임시 stack이 RSS의 얼마를
차지했는지는 이전 계측만으로 분리할 수 없다. 특정 allocation이나 allocator
동작을 확정하지 않는다.

종료 전 receipt의 `termination_audit.attempts`는 비어 있었고, 감독기 종료 사유도
없었다. 기록된 signal attempt 하나는 이미 child wait 상태를 관측한 뒤의
Drop 정리였으며, process group에 대한 SIGKILL이 `ESRCH`로 실패했다.
따라서 그 정리 호출을 최초 종료의 원인으로 설명할 수 없다. 외부 sender와
커널 OOM 여부를 확정할 추가 증거는 이 자료에 없다. 기존의 커널·audit 접근
제약과 구별해 기록하며, SIGKILL을 OOM 확정으로 치환하지 않는다.

## 2. 실제 소유 관계

| 저장소 | Owner와 공유 범위 | 이번 변경 이후의 수명 |
| --- | --- | --- |
| 요청의 원본 occupancy predicate | Coordinator Boolean arena | 각 rank의 임시 predicate를 rewind한 뒤에도 유지 |
| Rank의 공통 domain·board predicate와 기존 tables | `FrozenBooleanPrefix`의 단일 `Arc` | Worker들이 같은 prefix를 읽고, 모두 종료한 뒤 원래 owner로 이동 |
| Worker의 private decision nodes | 해당 piece의 `BooleanArena` | 읽기 전용 결과가 coordinator에서 소비될 때까지 유지 |
| Worker의 private unique/apply tables | 해당 piece의 `BooleanArena` | 모든 legal row root를 만든 직후 해제 |
| 완성된 legal row roots | `PieceMovement` | 해당 piece의 모든 joint 전이에 적용한 뒤 해제 |
| Joint ZDD와 supply/Hold roots | Coordinator | 기존 rank compaction 및 결과 수명 유지 |

`freeze_prefix`는 nodes, unique, applied를 `mem::take`하여 단일 `Arc`로 옮긴다.
각 worker마다 prefix의 Vec나 HashMap을 복제하지 않는다. Fork는 그 prefix를
읽고 자신만의 suffix와 table을 생성한다. Prefix owner의 ID를 읽을 수 있는지는
`can_read_owner`로 제한하고, piece relation의 local root는 정확히 그 fork
owner와 일치해야 한다. Sibling의 같은 숫자 ID를 같은 함수로 취급하지 않는다.
[Boolean owner 구현](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_boolean.rs),
[relation owner 검사](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_family.rs)

기존 rendezvous channel은 결과를 쌓는 별도 queue를 만들지 않지만, `send`를
기다리는 각 worker는 자신의 `PieceMovement`를 계속 소유한다. 따라서 channel
용량이 0이라고 모든 worker의 메모리가 하나로 제한되는 것은 아니다. 기존에는
여기에 더 이상 쓰지 않는 private unique/apply tables도 포함됐다.
[Worker와 소비 경계](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_parallel.rs)

이번 해제도 private node 전체의 live-root compaction은 아니다. 더 이상 legal
root에서 도달하지 않는 중간 decision node가 Vec 안에 남을 수 있으며, 그 node는
해당 결과를 소비하고 arena를 drop할 때 함께 해제한다. 여러 piece가 계산 중인
동안의 private suffix 중복과 active working set 역시 남아 있다.

## 3. Complemented edge로 함수와 부정 함수의 저장소 재사용

### 3.1 이전 표현에서 반복되던 계산

기존 `not(f)`는 `Xor(f, TRUE)`를 일반 apply로 수행했다. 새로운 부정 결과가
memo에 없다면 입력 DAG를 순회해 반대 극성의 decision node들을 intern하고,
그 apply 결과들도 기록했다. 이동 계산에서는 미방문 조건 `not(reached[target])`,
kick의 선행 실패 조건 `not(free[target])`, 아래 칸의 충돌 조건 등에 이 연산을
반복해서 사용한다. 같은 함수의 양쪽 극성이 필요한 경우 저장소가 분리되었다.
[이동 계산의 Boolean 호출](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_family.rs)

### 3.2 새 불변식

`BooleanId`의 하위 한 비트를 부정 여부로 사용한다. Terminal은 여전히 `FALSE=0`,
`TRUE=1`이고, decision node의 물리 slot과 root의 극성은 분리된다.
물리 slot \(i\)의 regular reference는 \(2(i+1)\), 반대 극성은 그 값의 하위
비트를 뒤집은 reference다. 같은 함수의 부정은 새 node를 요구하지 않는다.

`mk(variable, low, high)`는 low edge가 regular가 되도록 정규화한다. Low가
complemented라면 두 child의 극성을 모두 뒤집어 같은 tuple을 intern하고,
반환 root의 극성을 뒤집는다. 이는

\[
\operatorname{ITE}(x,l,h)
=\neg\operatorname{ITE}(x,\neg l,\neg h)
\]

라는 두 branch의 동시 부정 관계를 이용한다. 여기서 표기의 branch 순서는
`mk`와 같이 low, high다. 변수 순서, 같은 child의 축약, unique table의 exact
tuple equality는 유지한다. 따라서 고정된 변수 순서에서 root equality로 같은
Boolean 함수를 식별하는 기존 의미도 유지해야 한다.

`node(id)`는 stored tuple을 그대로 노출하지 않고 root의 극성을 두 child에
반영한 semantic node를 반환한다. Bridge의 missing-variable false 경로와
cofactor는 종전처럼 `node.low` 또는 `node.high`를 따라가면 된다.
Boolean root를 소비하는 다른 모듈은 numeric ID를 Vec index로 해석하지 않는다.
[표현과 node 조회](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_boolean.rs),
[Bridge 필터](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_bridge.rs)

### 3.3 연산과 memo 재사용

`not(f)`는 cancellation·root 유효성·work 한도를 확인한 뒤 한 번의 work 증가와
ID bit flip으로 끝난다. DAG 순회, node 생성, apply memo 삽입을 하지 않는다.
OR도 다음 항등식으로 같은 AND apply cache를 사용한다.

\[
f\lor g=\neg(\neg f\land\neg g).
\]

기존 apply key의 operation 종류를 제거하고 두 signed operand를 정렬한 key를
사용한다. Cofactor의 임시 memo도 regular root로 key를 정규화하고, 입력의
극성을 출력에 다시 반영한다. 함수와 그 부정에 대한 같은 제한 계산을 중복
저장할 필요가 없다. 이들은 큐 종류나 kick profile과 무관한 Boolean 항등식이다.

이 변경으로 모든 Boolean 계산이 상수 시간이 되는 것은 아니다. AND, cofactor,
이동 fixed point의 관계 폭은 계속 커질 수 있다. `node()` 조회에는 root 극성을
풀어주는 비용도 추가된다. 서로 다른 piece의 suffix가 같은 함수를 새로 만들면
현재 구조에서는 여전히 각 worker가 별도로 intern한다. 새 표현이 전체 입력에서
얼마나 유리한지는 측정 대상이며, 최악의 관계 폭에 대한 일반적인 축소 보장은 없다.

## 4. 완성된 worker 결과의 읽기 전용 경계

Worker가 모든 legal row predicate를 만든 뒤 coordinator의 `FamilyBridge::filter`는
그 arena의 nodes를 읽기만 한다. 이 지점에서 `seal_for_reading()`을 호출한다.
이 함수는 local unique/apply maps의 backing storage를 해제하고, 반환한 node
ID와 owner, 공유 prefix, 누적 work·cache hit·node peak는 유지한다. 반환값은
해제한 두 table의 payload 추정치다.

해제 뒤 다시 `mk`, `variable`, `and`, `or`, `not`, `ite`, `cofactor`, `freeze_prefix`,
`rewind`로 arena를 변경하려 하면 `InvalidInput("boolean_arena_sealed")`를 반환한다.
인덱스가 없어진 상태에서 같은 함수를 다른 ID로 생성하는 경로를 허용하지 않는다.
Seal을 다시 호출하면 추가 해제량 0을 반환한다. 공유 prefix의 tables는 다른
worker가 사용하므로 여기서 해제하지 않는다.

Coordinator의 단일 worker 경로는 rank 중간에 계속 predicate를 생성하므로
같은 방식으로 seal하지 않는다. Fork 수명 밖의 freeze/thaw/rewind와 owner
복귀도 유지한다. Cancellation 이후의 원래 `rewind` cleanup 경로는 계속
허용하되, 이미 seal된 arena의 재구성은 거절한다.

## 5. 자원 한도와 진단 값의 의미

`max_nodes`는 실제 저장된 decision nodes와 두 논리 terminal의 합을 센다.
부정 reference 자체는 저장 node를 추가하지 않는다. Work와 memo의 명시적
한도 및 `0=unlimited` 설정은 유지한다. 한계 도달, 취소, 잘못된 owner나 입력은
false predicate나 부분 성공으로 바꾸지 않는다. ID에 극성 한 비트를 쓰므로
물리 decision 주소는 31비트이며, 그 주소 공간을 넘으면 명시적 `NodeLimit`이다.

정확한 aggregate finite Boolean budget을 구현하지 않은 병렬 경로는 여전히
명시적 node/work/memo 한도가 모두 unlimited일 때만 사용한다. 그 외에는 기존
sequential fallback을 보고한다. 사용자의 worker 설정을 몰래 줄여 동일한
budget인 것처럼 보이게 하지 않는다.

새 read-only accessor는 private nodes, unique table, apply memo의 추정 용량을
따로 제공한다. `private_node_count()`는 실제 local decision node만 세며,
공유 prefix와 두 논리 terminal을 제외한다. 기존 `node_count()`와 node peak는
접근 가능한 decision storage에 두 terminal을 더한 값이다.

기존 \(2^{18}\) work pulse마다 `symbolic_storage_boolean_private_nodes`와
`symbolic_storage_boolean_private_bytes`를 보낸다. Seal 직후에도 값을 갱신한다.
Worker adapter가 piece별 고정 stage로 전달하고, 별도 sampler가 시간을 읽고
RSS와 함께 출력한다. Core에는 clock, 문자열 formatting, 파일 I/O를 추가하지
않는다. 생성·전파·lock·legal row·publication·소비·해제 phase도 독립적으로 표시한다.
[이동 진단 adapter](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_movement_progress.rs)

HashMap payload 추정치는 allocator의 실제 bucket backing과 alignment, 순간적인
rehash 공존, apply/cofactor stack을 모두 포함하는 값이 아니다. 공유 prefix는
worker마다 더하지 않는다. 해제 후 allocator가 주소 공간이나 물리 page를 언제
반환하는지도 별개이므로, released bytes를 곧바로 RSS 감소량으로 해석하지 않는다.

## 6. 기존 이동 알고리즘의 증명과 계산 재사용

기존 symbolic 경로에도 rank domain 제한과 `pending/delta/novel` 증분 전파는
있었다. 이번에 이를 새 최적화라고 다시 주장하지 않는다. 실제로 빠져 있던
재사용은 scalar 이동 template가 이미 컴파일한 **불가능한 회전 간선 제거와
endpoint 조건 아래의 first-success guard 축약**이다.

Pose \(p\)의 footprint를 \(M_p\), cell 점유 predicate를 \(O_c(S)\), 현재
선택 집합의 domain을 \(D(S)\)라 하면 collision-free 조건은 다음과 같다.

\[
F_p(S)=D(S)\land\bigwedge_{c\in M_p}\neg O_c(S).
\]

순서가 있는 kick 후보 \(t_i\)로 가는 간선의 정확한 조건은

\[
F_s\land F_{t_i}\land\bigwedge_{j<i}\neg F_{t_j}
\]

이다. Earlier target이 이미 방문되었거나 그 target에서 해법이 없다는 이유로
later kick을 허용하지 않는다. First success는 해당 target의 collision-free
조건으로 결정된다.

`ReachabilityTemplate`의 기존 reverse CSR에는 모든 점유에서 불가능한 간선이
이미 제거되어 있다. 예를 들어 earlier footprint가 source와 target의 합집합
안에 있으면, 양 endpoint가 비었을 때 그 earlier candidate가 반드시 먼저
성공하므로 later edge는 사용할 수 없다. 새 symbolic 코드는 이 증명을 다시
만들지 않고 `(source, slot, original ordinal)`로 기존 descriptor를 조회한다.

Endpoint가 비었다는 전제 아래 prior collision clause는 footprint에서
\(M_s\cup M_{t_i}\)를 제거해도 같은 답이다. 기존 descriptor가 이를
`NO_PRIOR`, `SINGLE_BIT`, `SINGLE_MASK`로 증명한 경우만 축약한다. 단순히
여러 clause의 intersection을 새 조건으로 추정하지 않는다. 실제로 모든 다른
clause에 포함되는 prior clause가 없으면 `ORDERED`를 사용한다.

조회는 target별 incoming degree에 대한 binary search다. `ORDERED`는 transition별
lazy cursor로 **원본** prefix를 한 번씩 전진하므로, 길이 \(k\)인 table의 prior
결합을 candidate마다 처음부터 반복하는 \(O(k^2)\) 방식이 아니다. 제거된 간선도
이후 first-success 조건의 earlier competitor에서는 빠지지 않는다. 큐 이름이나
kick profile 이름에 따라 결과를 특별 처리하는 분기는 추가하지 않았다.
[기존 template와 guard 증명](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability.rs),
[symbolic 적용](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_family.rs)

Row availability도 frame별로 반복 결합하지 않고 다음 동치에 따라 마지막에
한 번 적용한다. \(A_r=D\land\neg selected(r)\), \(B_f\)는 deleted-row frame,
\(L_f\)는 해당 frame의 정확한 grounded reachable lock 조건이다.

\[
\bigvee_f(A_r\land B_f\land L_f)
=A_r\land\bigvee_f(B_f\land L_f).
\]

`A_r == FALSE` 조기 반환은 유지한다. 이 변경은 gate 호출 수를 줄이지만, gate를
뒤로 옮기면서 중간 식이 커질 수도 있다. 따라서 각 변경의 독립적인 속도 기여를
통합 후보의 단일 실행으로 확정하지 않는다.

새 work counter는 실제 candidate 확인, template에서 제거된 candidate,
guard 4종의 사용, 원본 ordered prior term의 결합 수를 구분한다. 완주 보고서에서
확인하는 누적 work이며, 실패한 실행에 최종 집계가 존재하는 것처럼 보충하지 않는다.

### 6.1 첫 실측 이후: domain·source 조건의 반복 결합 제거

첫 후보는 rank 3에서 대부분의 worker가 Free를 구성하는 동안 끝났다. 도달
전파나 Build 처리의 최적화만으로 이 앞단의 소유 비용을 줄일 수 없으므로,
충돌 predicate와 현재 도달 집합의 domain을 별도로 취급한다.

\(C_p=\bigwedge_{c\in M_p}\neg O_c\)만 Free 배열에 저장하고, seed를
\(D\land C_{seed}\)로 제한한다. Target-free를 포함하는 간선만 전달하므로
다음 귀납 불변식이 유지된다.

\[
reached_p,\ pending_p\subseteq D\land C_p.
\]

각 delta 자체가 \(D\)와 source-free를 만족한다. 따라서 translation과
NO_PRIOR에는 \(C_{target}\)만 필요하고, 다른 guard도 target-free와 기존 prior
조건만 결합하면 된다. ORDERED prefix는 TRUE에서 시작한다. Endpoint 조건을
사용한 기존 guard 축약의 source-free 전제는 delta가 충족한다. Grounding은
domain 안의 reached에 적용하고, legal row의 최종 availability도 유지한다.

\(C_p=\neg D\)인 경우에는 domain 안에서 항상 막힌 pose이므로 FALSE로
정규화한다. 이 정규화는 domain 밖 함수값을 보존할 필요가 없다는 계약을 사용한다.
빈 domain도 빈 도달 집합으로 남는다. 물리 보드를 열거하거나 큐 종류에 따라
특수 처리하는 경로는 추가하지 않았다.

Source-free를 guard에서 미리 결합하지 않으면, 일부 비호환 간선의 제거가 실제
delta 적용 때로 이동할 수 있다. 따라서 Free 구성 절감과 함께 retained edges,
propagation work 및 실제 저장소를 봐야 한다. 도달 관계 자체의 폭을 없앴다고
주장하지 않는다. 이 변경에 대한 raw reached cycle fixture와 모든 builtin/imported
lock의 \(lock\land\neg D=FALSE\) 검사는 최종 availability gate가 오류를 가리는
것을 방지한다.

### 6.2 최종 후보: Boolean 간선 guard bank를 생성하지 않는 전파

두 번째 후보의 Edges 성장을 근거로, 모든 간선의 Boolean 조건을 먼저 만드는
단계를 없앴다. `compile_transfers`에는 `BooleanArena` 인자가 없다. 이 함수는
template의 exact proof를 다음 고정 크기 descriptor로 옮기며 Boolean AND/NOT를
실행하지 않는다. 현재 x86_64 시험에서 target과 guard를 합친 `Edge`는 24 bytes다.
Vec의 capacity, offsets 등 별도 저장소는 여기에 포함하지 않는다.

| Descriptor | 실제 delta에 적용하는 조건 |
| --- | --- |
| TargetFree | \(\Delta\land C_{target}\) |
| SingleCell | 위 조건과 기존 단일 cell 점유 조건의 교집합 |
| BlockedPriorPose | 위 조건과 지정된 prior pose의 충돌 조건의 교집합 |
| Ordered | 위 조건과 같은 slot의 모든 원본 earlier candidate 실패 조건의 교집합 |

Source를 queue에서 꺼낼 때마다 \(\Delta\)에 대한 slot별 prefix를 만든다.

\[
P_k=\Delta\land\bigwedge_{i<k}\neg C_i,
\qquad contribution_k=P_k\land C_k.
\]

\(P_k\)에는 현재 target의 free 조건을 넣지 않는다. 그래야 현재 target에서
배제된 assignment를 이후의 다른 target까지 잘못 제거하지 않는다. Slot마다
독립적인 cursor와 remaining을 두고, 같은 source에 새 delta가 도착하면 모두
초기화한다. Descriptor가 없거나 현재 candidate의 교집합이 비어 있어도,
나중의 ordered prefix는 원래 forward list의 앞선 후보를 빠짐없이 읽는다.
이미 도달한 target이라는 이유로 first-success 경쟁자를 제거하지 않는다.

이 구조에서 보관하는 간선 명세의 크기는 간선 수에 선형이다. 도달하지 않은
조건에 대한 전역 Boolean guard family는 생성하지 않는다. 다만 실제 delta에
조건을 적용하면서 만든 Boolean 중간 nodes와 apply memo는 arena에 누적된다.
Source-pop이 반복되면 delta별 조건 계산도 반복된다. 각 slot의 cursor는
prefix를 한 번씩 전진하지만, 전체 pop 횟수까지 상수로 바뀌는 것은 아니다.

`guarded_edges`/`guarded_movement_edges`는 최종 후보부터 **retained descriptor 수**다.
이전 후보의 미리 계산한 nonfalse guard 수와 직접 비교하지 않는다.
`rotation_ordered_prior_terms`도 실제 전파 중 delta-local prefix에 적용한 항 수다.
새 fixture는 16개 assignment, 두 회전 slot, 재queue된 두 delta, 제거된 earlier
competitor, 모든 descriptor 종류를 비교한다. 예상 prior 적용 수 6회도 확인했다.

## 7. 검증과 후속 계측

Boolean 의미 검사는 세 변수의 모든 진리표와 cofactors, 두 변수의 모든
AND/OR/ITE 조합, 부정 시 node/memo 증가 없음과 work 한 번, node/work/memo
refusal, 31비트 주소 경계, fork owner, seal 후 읽기와 재구성 거절, 취소 후 rewind를
포함한다. 깊은 apply가 재귀 stack을 사용하지 않는 기존 fixture도 유지했다.
이는 유한한 의미 검사이며 작은 입력의 성능 벤치마크가 아니다.

| 실제 검증 | 결과 | 원시 기록 |
| --- | --- | --- |
| Boolean 표현·owner·한도 | 9/9 PASS | `symbolic-owner-boolean-tests-r2-20261123.log`, receipt `1795428872237036842-3` |
| 첫 후보 이동·Hold·source·parallel·archive | 30/30 PASS | `symbolic-owner-family-tests-r2-20261123.log`, receipt `1795428946369232137-3` |
| 두 번째 domain/source 변경을 포함한 같은 범위 | 31/31 PASS | `symbolic-domain-transfer-tests-20261123.log`, receipt `1795429614891333667-3` |
| 최종 transfer descriptor 변경을 포함한 같은 범위 | 32/32 PASS | `symbolic-transfer-descriptor-tests-20261123.log`, receipt `1795430299844999146-3` |
| Manager 종료·소유권 Rust fixture | 5/5 PASS | `termination-owner-tests-20261123.log` |
| 실제 manager 프로세스 수명 fixture | 5/5 PASS | `termination-owner-lifecycle-20261123.log` |
| 릴리스 helper ESRCH·불명 상태 fixture | 3/3 PASS | `termination-release-helper-tests-20261123.log` |
| 변경 Rust rustfmt 및 git diff 공백 검사 | PASS | 변경 파일만 검사, unrelated formatting은 변경 안 함 |

경로 접두사는 모두 `_local/artifacts/`다. 30/31/32개 core filter는 반복되는
검증이므로 합산해 독립 검증으로 세지 않는다. 최종 선택 범위는 Boolean 9,
family 32, 종료 경계 13의 **서로 다른 54개**다. 변경된 부분에 필요한 로컬 검증이며,
workspace 전체 CI나 Windows 실행을 완료했다고 주장하지 않는다.

첫 Boolean 시험 빌드에서는 새 fixture closure의 정수 추론 오류를 명시적 `u32`로
고친 뒤 다시 빌드했다. 첫 30개 family 시험의 동시 실행은 기존 참조 executor의
shared resource admission 경합으로 하나가 거절됐다. 정확성 사례들만
`--test-threads=1`로 직렬 실행하여 모두 통과했고, 병렬 solver fixture의 worker
정책과 실제 P7P4의 all-9 설정은 변경하지 않았다. 실패 로그도
`symbolic-owner-boolean-tests-20261123.log` 및
`symbolic-owner-family-tests-20261123.log`에 보존했다.

이동 단계의 active private arena와 publication을 기다리는 완료 arena를 구분한다.
Seal은 후자의 table 수명을 줄이고 complemented edge는
전자에서도 부정에 의한 중복을 줄인다. 그래도 single piece의 관계 자체가 커질 수
있으므로, 이 두 변경만으로 Full4 또는 Full6의 완료를 예상하지 않는다.

## 8. 첫 후보의 실제 P7P4 계측

`full4-symbolic-ownership-20261123` 실행은 새 manager를 사용했다.

| 항목 | 실제 기록 |
| --- | --- |
| 실행 source commit | `a4f264363e80334154cbf0c39e2c74f5c4d3ac08`의 수정 작업트리 |
| 컴파일에 기록한 source tree | `cee6f398c04a71f280077647786735dcfcf732b9` |
| Solver SHA-256 | `26b67ead8267d5a1b2509063d193dc8105a0e0cf861a09aeb954fa428cbda4d9` |
| Manager SHA-256 | `f722726c2acc8f99101cb7cf57e4eede75d0b187e2680fe03e96a83dc1828385` |
| Runtime receipt | `1795429158707501894-3` |
| Request fingerprint | `e709af430f47ba7292eba224b3af7414efaa13560082dc77c474089992afa924` |
| Source fingerprint | `989d0aa152b2a75913d02142fcca6df8d6e88302de116c84060fb22decc81d56` |
| Worker policy | 사용 가능한 논리 CPU 9개 전체, 실제 작업 수 외 임의 축소 없음 |
| Search timeout / implicit deadline | `0` / `null` |
| Rank 2 완료 첫 표본 | 34.029395055 s |
| 마지막 표본 | 41.042061525 s, rank 3 movement |
| Manager 경과 시간 / 종료 | 42.328 s / Unix signal 9, code 137 |
| Child 표본 VmHWM | 2,914,856,960 bytes |
| Manager 표본 process-group peak | 2,525,061,120 bytes |
| Wait 전 / 후 내부 종료 요청 | `[]` / `[]` |
| Pressure events / 자동 retry | 0 / false |
| 탐색 완료 / archive | false / accepted-result archive 생성 안 됨 |

입력은 기존 empty 10×4, fresh Standard7Bag 11 draws P7P4, 빈 enabled Hold,
Jstris180, 10 locks와 같다. 원본 source 4,233,600개를 압축 표현하며 probability는
omit이다. 두 fingerprint가 이전 piece-cache 실행과 일치한다. Worker나 예산을
변경한 OOM 재시도는 하지 않았다.

마지막 표본의 private payload는 다음과 같다. 이 값들은 각 worker가 저빈도
pulse로 보낸 최신 추정치이며, 정확히 같은 시각의 allocation 또는 RSS가 아니다.

| Piece | Phase | Private nodes | Private retained estimate, bytes |
| --- | --- | ---: | ---: |
| I | Free | 4,062,395 | 270,532,608 |
| O | Edges | 3,661,066 | 208,142,336 |
| T | Free | 5,266,923 | 416,284,672 |
| S | Edges | 5,463,787 | 416,284,672 |
| Z | Free | 4,177,008 | 270,532,608 |
| J | Free | 4,337,437 | 320,864,256 |
| L | Free | 4,294,810 | 320,864,256 |

공유 prefix의 값은 별도로 803,554 logical nodes, 123,600,896 bytes였다.
이 단계의 rank 3 worker는 아직 seal 경계에 도달하지 않았다. 반면 rank 2의
각 worker에서 관측한 해제 payload는 I/S/T/Z 각 31,653,888 bytes,
J/L 각 63,307,776 bytes, O 4,931,584 bytes였다. 이는 완료된 builder maps의
소유권이 실제로 끝났다는 계측이며, 같은 크기의 RSS 감소를 뜻하지 않는다.

새 manager의 요청 audit와 후속 stderr event가 모두 비어 있으므로, 이제는 이전
후보에 있던 post-wait Drop SIGKILL도 없다. **그럼에도 raw signal 9가 재발했다.**
내부의 과잉 정리 결함을 고친 것과 현재 종료의 발신자 확인은 분리해야 한다.
호스트의 OOM 기록 또는 발신자 tracing은 여전히 확보되지 않았다.
[전체 종료 감사와 실제 경계 검증](2026-11-23-termination-ownership.md)

원시 자료: `_local/artifacts/full4-symbolic-ownership-20261123`의 `.log`,
`.summary.json`, `.runtime.json`, `.observation.json`, `.movement-observation.json`.
관측 JSON은 Rust helper로 추출했으며 1초 표본 사이의 phase·peak를 복원하지 않는다.

첫 후보 release build는 기존 thin LTO, codegen-units=1, default incremental을
유지했다. `--timings`에서 131.8 s, 60 fresh / 4 dirty units, executor 98.09 s
(frontend 18.99 s, codegen 79.10 s), compare binary 33.57 s였다. 이는 변경된
소스의 빌드 기록이며 이전 빌드와 같은 입력의 개선율 비교가 아니다.
근거는 `_local/artifacts/cargo-timings-owner-20261123.json`과
`symbolic-owner-release-build-20261123.log`, receipt `1795428989423860078-3`다.

## 9. Domain/source 반복 결합 제거 후보의 실제 P7P4

`full4-symbolic-domain-transfer-20261123`은 같은 두 fingerprint와 입력 객체,
all-9 worker 정책, 시간 제한 없음으로 실행했다. 결과는 **46.053초 SIGKILL,
rank 3 미완료**다. 이전 후보와 도달한 계산량이 달라 경과 시간의 비율을
완료 성능 개선율로 쓰지 않는다.

| 항목 | 실제 기록 |
| --- | --- |
| Compiled source tree | `5e5f5edc6045dd8d43d02ee6c397579a31de55ad` |
| Solver SHA-256 | `222eb84120b9172928683dba0d14c4c22d51c319ba1d6da27293c3faa86e42ea` |
| Receipt | `1795429836851092198-3` |
| Rank 2 완료 첫 표본 | 35.030747729 s |
| 마지막 표본 | 45.051547623 s, rank 3 row_down, O row 38 |
| Child 표본 VmHWM | 2,979,282,944 bytes |
| Manager 표본 process-group peak | 2,871,218,176 bytes |
| Wait 전 / 후 내부 종료 요청 | `[]` / `[]` |
| Timeout / pressure events / automatic retry | 0 / 0 / false |

마지막 표본에서 I/T/S/Z/J/L은 모두 Edges에 있었고, O는 Consuming이었다.
즉 모든 piece가 Free를 통과했으며, O는 propagation·locks·legal rows도 마친
후 coordinator가 그 결과를 실제 joint relation에 적용하고 있었다.
O의 private builder tables는 **253,231,104 bytes 추정치가 해제**됐고,
읽기용 node storage 50,331,648 bytes가 남았다. 나머지 여섯 private arena는
303,562,752~416,284,672 bytes 추정치까지 커졌다.

Domain/source를 모든 pose와 edge에 결합하던 중복은 줄었지만, 전역 Boolean
edge guard를 미리 만드는 비용이 뒤에서 다시 커졌다. 이 결과는 Free 이후
진전의 증거이며, 전체 peak 감소나 Full4 완주의 증거가 아니다. 특히 coordinator의
row_down과 남은 worker의 guard 작성이 동시에 진행하므로 마지막 전역 phase
하나만 보고 전체 메모리를 compatibility filter에 귀속하지 않는다.

이 후보의 `--timings`는 compare binary만 대상으로 116.5 s, 60 fresh / 2 dirty
units였다. Executor 84.24 s 중 frontend 6.13 s, codegen 78.11 s, compare binary
32.19 s였으며 release 최적화 옵션은 그대로다. 소스와 build target 집합이 다른
앞 후보와의 빌드 속도 비율은 제시하지 않는다.

원시 자료는 `_local/artifacts/full4-symbolic-domain-transfer-20261123`의
`.log`, `.summary.json`, `.runtime.json`, `.observation.json`,
`.movement-observation.json`과 `cargo-timings-domain-transfer-20261123.json`이다.
Build receipt는 `1795429710269470711-3`다.

## 10. 최종 transfer descriptor 후보의 실제 P7P4

최종 source는 `full4-symbolic-transfer-descriptor-20261123`으로 계측했다. 세 후보의
compiled request 객체, source 객체, 두 fingerprint가 일치함을 별도로 확인했다.
알고리즘 수정 외에 worker 수, timeout, source horizon, kick, Hold 또는 예산을
바꾸지 않았다. Source commit 필드는 셋 모두 기존 HEAD `a4f264363e80334154cbf0c39e2c74f5c4d3ac08`이며,
후보의 실제 코드는 각 source tree와 binary hash로 구별한다.

| 항목 | 실제 기록 |
| --- | --- |
| Compiled source tree | `4708f8d44f59926f7d3acfd5e99d38a66d8b09c1` |
| Solver SHA-256 | `b3d9020f42fada679f79aafb5fc149a278d3d6a8b9bc276f741f585fa0744dfc` |
| Runtime receipt | `1795430577350827138-3` |
| Geometry compiler / certificate | 1.540252865 s / 0.002347505 s |
| Rank 2 완료 첫 표본 | 42.055477172 s |
| 마지막 표본 | 50.064577133 s, rank 3 movement |
| Manager 경과 시간 / 종료 | 51.463 s / Unix signal 9, code 137 |
| Child 표본 최대 RSS / VmHWM | 2,583,478,272 / 2,697,310,208 bytes |
| Manager 표본 process-group peak | 2,755,104,768 bytes |
| Wait 전 / 후 내부 종료 요청 | `[]` / `[]` |
| Search / identity export / coverage export | 모두 미완료 |

마지막 표본에서 **일곱 미노 모두 Propagation**이었다. Rank 3의 publication이나
consumption은 관측하지 못했다. Private suffix 추정치는 I/O/Z 303,562,752 bytes,
T/S/L 208,142,336 bytes, J 365,953,024 bytes였고, 공유 prefix는 앞 후보들과 같은
123,600,896 bytes였다. Parent private는 freeze 중 0으로 보고됐다. Boolean work는
282,802,588이었으며 coordinator의 bridge/compatibility work는 그 구간에서 멈춰 있었다.
이는 최종 후보의 남는 저장소가 **실제 delta 전파의 중간 노드·메모**라는 근거이며,
각 private payload가 RSS 전체를 정확히 분해한다는 뜻은 아니다.

세 후보를 함께 보면 다음과 같다.

| 후보 | Rank 2 완료 최초 표본 | 강제 종료까지 경과 | 마지막 rank 3 상태 |
| --- | ---: | ---: | --- |
| Complemented edge + seal + 기존 guard 재사용 | 34.0294 s | 42.328 s | 5개 Free, O/S Edges |
| Domain/source 반복 결합 제거 | 35.0307 s | 46.053 s | O Consuming, 나머지 6개 Edges |
| Boolean guard bank 제거, delta-local 적용 | 42.0555 s | 51.463 s | 7개 모두 Propagation |

**최종 후보의 rank 2 완료는 더 늦게 관측됐다.** 제거한 준비 계산과 동적으로
반복하는 조건 계산 사이의 tradeoff를 포함해 전체 성능을 더 검증해야 한다.
51.463초를 완주 시간으로 사용하거나, 중간 도달 단계의 변화만으로 전체 개선을
선언하지 않는다. 마지막 child RSS 표본이 앞 후보보다 낮더라도 두 번째 후보의 O는
이미 소비 중이었고 마지막 후보는 전원이 전파 중이므로 일반적인 절감률로 환산하지
않는다. 구조적으로 제거한 전역 guard 준비 비용과 남는 전파 비용을 구분한다.

이번 실행도 `supervisor_termination_reason=null`, timeout 0, pressure events 0,
`automatic_retry=false`, 미기록 signal attempt 0이었다. 확인된 빈 그룹의 중복
정리 제거는 세 실행에서 작동했으며, 명시적인 내부 종료 요청 없이 SIGKILL이
재발했다. 발신자와 커널 OOM 여부를 확정할 호스트 증거는 여전히 없다.

최종 release build는 147.2 s, 60 fresh / 2 dirty units였다. Executor 108.38 s
(frontend 16.82 s, codegen 91.56 s), compare binary 38.59 s이며 thin LTO,
codegen-units=1, default incremental은 바꾸지 않았다. `--timings`는 executor의
codegen과 최종 binary 단계가 여전히 대부분의 빌드 비용임을 보여 준다.
Build receipt는 `1795430396872105405-3`다. 이 기록 역시 서로 다른 source revision의
관측이며, 같은 입력의 build 속도 개선율은 아니다.

원시 자료는 `_local/artifacts/full4-symbolic-transfer-descriptor-20261123`의
`.log`, `.summary.json`, `.runtime.json`, `.observation.json`,
`.movement-observation.json`과 `cargo-timings-transfer-descriptor-20261123.json`이다.
최종 측정 뒤 executable source는 바꾸지 않고 이 결과와 종료 감사 문서만 갱신했다.

## 11. 남은 구현 작업과 이번 종료 범위

이번 변경은 `symbolic-family` 연구 경로의 표현·계산·소유권 개선이며 제품 기본
dispatch 전환은 하지 않았다. 메모리를 줄이기 위해 worker를 제한하거나, 큰 실행의
실패를 빈 성공 결과로 바꾸는 처리는 없다. Full4가 완료되지 않았으므로 전체 해법
집합의 기존 경로 대조와 accepted CTK4 생성은 아직 완료되지 않았고, Full6도 실행하지
않았다. CI 실행·조회·대기는 계속 보류했다.

다음 구현 검토의 우선순위는 실제로 남은 전파의 live working set과 rank 2의
compatibility 비용이다. 일반적인 계산 재사용과 정확한 수명 경계를 기준으로 한다.

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_boolean.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_family.rs`, 구현 내용: free/reached/pending 및 최종 legal root의 생존 범위를 기준으로 private intermediate node 회수 경계를 설계하고, root remap·정규성·취소·compaction 순간의 추가 메모리를 검증한다. 무조건적인 memo 비우기로 재계산 비용을 전가하지 않는다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_family.rs`, 구현 내용: 각 piece가 실제 공급되는 ready relation의 row domain을 공유·재사용하여 불필요한 이동 계산을 줄이고, domain 생성 비용과 shared prefix 증가를 함께 계측한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_compatibility_filter.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family.rs`, 구현 내용: 여러 supply state에서 반복되는 같은 row-prefix의 기하 필터를 묶고 원본 joint relation에 semijoin하는 재사용 범위를 검토하여, rank 2의 반복 compatibility work를 줄인다.
- [ ] 수정 파일: `tools/clearra-manage/src/runtime.rs`, 구현 내용: 최초 leader reap부터 group cleanup까지의 숫자 PID/PGID 재사용 경쟁을 닫을 수 있는 안정된 OS 소유권과 종료 관측 순서를 설계하고, 기존 비상 tree cleanup을 포함해 검증한다.

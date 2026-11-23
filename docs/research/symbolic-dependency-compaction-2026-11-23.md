# 공급·기하 의존성의 공유 계산과 Boolean 중간 노드 회수

작성일: 2026-11-23. 기준 commit: `e01c33c1be16269bcfdc755cd54d56b13b1c9f99`.

## 1. 범위와 확인된 결과

이번 변경은 `symbolic-family` 연구 경로에 적용했다. 제품 기본 dispatcher를
전환하지 않았으며, 완성된 기하 DAG의 path를 열거하거나 candidate별 Build graph를
만드는 경로를 추가하지 않았다. 원래 큐와 Hold의 연결 관계를 유지한 채 반복되는
기하·이동 계산의 입력을 묶고, worker가 더 이상 쓰지 않는 중간 predicate를 회수한다.

수정 후 선택한 로컬 검증은 **Boolean 14개와 family 38개, 총 52개가 통과**했다.
작은 입력은 정확성 fixture로만 사용했다. CI 실행·조회·대기, 알려진 100% 확률 명제의
추가 검색·탐색·증명은 하지 않았다. 모든 본 실행은 manager의 `--no-timeout`으로
수행하며, 전체 논리 CPU 선택과 준비된 worker의 즉시 시작 정책을 유지했다.

회수 기능을 넣기 전의 첫 후보는 P7P4 rank 2 완료를 10.011775475초 표본에서
처음 관측했다. 이전 후보의 42.055477172초보다 빠르지만, 두 실행 모두 rank 3에서
SIGKILL로 끝났으므로 전체 완주 속도 개선을 뜻하지 않는다. 최종 회수 후보의 실제
실행 결과는 아래 실측 절에 별도로 기록한다.

환경·종료 원인의 확인 범위는 [저장소·프로세스 감사](2026-11-23-storage-process-pressure.md),
이전 Boolean 표현은 [complemented-edge 및 소유권 기록](symbolic-ownership-algorithm-reuse-2026-11-23.md),
다음 표현 변경안은 [occupancy quotient 설계](symbolic-occupancy-quotient-design-2026-11-23.md)에 있다.

## 2. 원본 상관관계를 유지하는 공급·기하 계산

미노 종류를 p, 공급/Hold 상태를 s, 현재까지 선택한 배치 집합을 X, 원래 큐를 Q라
한다. 공급 전이 후 joint relation을 R[p,s](X,Q)로 두고 다음 집합을 한 번 만든다.

\[
P_p=\bigcup_s\pi_X R_{p,s}.
\]

물리 이동은 P_p의 Boolean characteristic domain에서만 계산한다. 원래
R[p,s]는 별도로 유지한다. 같은 P_p를 가진 미노는 canonical ZDD root를 key로
domain 변환 결과도 공유한다. 현재 rank 전체 domain도 먼저 row prefix를 project한
후 합집합하므로, 물리식에 불필요한 source suffix를 함께 합치지 않는다.

이 domain은 base board의 domain과 다시 교집합한다. 기존 occupancy, 삭제 row,
ILC frame 식은 공유하며 새 board를 복사하거나 미노마다 재생성하지 않는다. 공유
occupancy는 더 넓은 base domain 밖에서만 0이라는 기존 의미를 유지한다. 좁아진
piece domain 밖의 값을 빈 보드로 가정하지 않는다.

후보 배치 row r의 정확한 이동 판정을 L_r, 전체 기하 family의 하향 closure를 Down(F),
집합에 r을 추가하는 함수를 add_r라 하면 계산 순서는 다음과 같다.

\[
A_r = P_p\cap L_r,\qquad
E_r = \mathrm{add}_r(A_r)\cap\mathrm{Down}(F).
\]

기존에는 L_r과 기하 판정을 공급 상태별로 반복했다. 이제 A_r과 E_r을 row 하나당
한 번 계산한 뒤, 원래 joint relation에 정확한 prefix semijoin을 적용한다.

\[
R'_{p,s,r}
=\mathrm{add}_r\left(R_{p,s}\ltimes\mathrm{remove}_r(E_r)\right).
\]

`legal_row`가 이미 `!selected(r)`을 포함하므로 이 구간의 add/remove는 일대일
대응이다. 서로 다른 Q를 별도로 project한 뒤 X와 다시 곱하지 않는다. 따라서 같은
보드를 공유하는 큐들의 tail, Hold 상태, EOF의 마지막 Hold release를 섞지 않는다.
`allowed == extended`이면 remove 대신 원래 A_r을 재사용한다.

이것은 불완전한 Build 선후관계 추정으로 해를 버리는 pruning이 아니다. 기존의
정확한 Down(F), legal-row 판정과 공급 전이를 집합 항등식에 따라 먼저 계산하고
공유하는 구조다. 기하·source predicate의 교집합과 projection을 commute할 수
있는 범위만 이동했다. 모든 resource refusal는 EMPTY나 성공으로 바꾸지 않는다.

추가한 진단값은 다음 의미다.

| 값 | 의미 |
| --- | --- |
| `ready_row_projections` | 공유 piece family를 준비하며 수행한 ready source root projection 호출 수 |
| `piece_domain_builds` | 공통 rank domain을 제외하고 추가로 변환한 서로 다른 piece domain 수 |
| `source_semijoins` | 공유 row 결정을 원래 joint relation에 적용한 실제 semijoin 호출 수 |
| `geometry_filters` | 이제 공급 상태별 반복이 아닌 공유 row family에 대한 기하 filter 호출 수 |

## 3. 실제 경로가 있는 낙하 증명과 guard 의존성

기존 immutable sky seeds, downward translation, pose collision predicate C를
재사용한다. general fixed point 전에 위에서 아래로 한 번 순회하며 다음을 계산한다.

\[
V_{seed}=D_p\cap C_{seed},\qquad
V_{down}\mathrel{\cup=}V_{source}\cap C_{down}.
\]

모든 V의 원소에는 실제 sky에서 아래로 내려온 경로가 있으므로 V는 전체 Reach의
부분집합이다. 이 결과로 reached와 pending을 초기화한 뒤 모든 translation과
ordered kick을 포함하는 기존 fixed point를 계속한다. 직선 증명의 실패는 unknown이며
불가능 판정으로 쓰지 않는다. 별도의 column BDD bank나 지속하는 certificate 배열은
만들지 않고 이미 필요한 reached 배열을 사용한다.

목적지에서 이미 증명한 집합 R_t는 복잡한 guard를 적용하기 전에 제외한다.

\[
(\Delta\cap G)\setminus R_t
=(\Delta\setminus R_t)\cap G.
\]

Ordered kick의 선행 실패 prefix는 여전히 target과 무관한 원래 delta에서 시작한다.
Target-free 또는 target-unseen으로 좁힌 결과를 그 공유 prefix에 넣지 않는다. 원본
kick 순서, 제거된 earlier candidate의 차단 조건, slot별 독립성도 유지한다. 모든
기본 프로필과 실제 imported ordered table에 적용되며 특정 SRS나 큐에 한정하지 않는다.

`vertical_seed_edges`, `vertical_seed_updates`, `early_reached_rejections`로 추가된
증명 비용과 생략된 guard 평가를 구분한다. 새로운 prepass가 모든 입력에서 이득이라는
보장은 없으며, 실제 전체 시간의 변화는 측정 대상이다.

## 4. 살아 있는 root만 보존하는 private arena 회수

`BooleanArena::compact_private`는 immutable prefix를 가진 worker fork의 suffix를
대상으로 한다. 호출자가 이후 다시 사용할 모든 private root를 전달해야 한다.
prefix의 모든 root는 자동으로 고정하며 숫자 ID, node 배열, unique/apply table을
이동하거나 복제하지 않는다.

동작은 다음과 같다.

1. 기존 private node마다 u32 relocation 항목 하나를 예약한다.
2. caller root를 표시하고, child-before-parent 생성 순서를 이용해 역순으로 자식까지
   표시한다. 별도 graph 복사나 DFS stack은 없다.
3. 생존 node에 새 정규 ID를 부여한다. complemented edge의 polarity는 별도로 보존한다.
4. 필요한 capacity, work 한도, owner 발급, cancellation을 재배치 전에 확인한다.
5. 기존 node Vec 안에서 앞으로 이동하고 같은 unique HashMap을 clear/rebuild한다.
   commit에는 추가 할당이나 observer callback, fallible 반환을 넣지 않는다.
6. 전달된 root를 remap하고 fork owner를 갱신한다. 이전 checkpoint와 relation owner는
   거절되며, 고정된 prefix owner는 계속 읽을 수 있다.

Apply cache 전체를 무조건 비우지는 않는다. 두 입력 ID가 그대로인 살아 있는 key와
살아 있는 result는 유지한다. result만 이동했으면 value를 remap한다. 입력이 이동했거나
result가 죽은 항목은 버리고 실제 폐기 수를 기록한다. 이를 유지하려고 두 번째 큰
HashMap을 만들지 않는다. 버린 항목의 재계산 비용은 남는다.

초기 성장 trigger는 private node 262,144개다. 이후에는 마지막 생존 크기 또는
scan 크기에 따라 다음 검사를 뒤로 미룬다. 죽은 node가 `ceil(N/8)`보다 적으면
재배치를 생략하고 기존 ID와 cache를 보존한다. 이 숫자는 admission 한도나 worker
감축이 아니라 반복 scan 비용을 상각하는 정책이다.

비용은 `O(private nodes + root slots + unique/apply capacity)`이며 relocation
scratch는 private node마다 u32 한 개를 요청한다. Vec와 두 map의 backing capacity를
재사용하므로 논리적 회수가 즉각적인 RSS 반환을 뜻하지 않는다. Work 한도나 cancellation
등으로 commit 전에 실패하면 roots/nodes/maps/owner의 의미 상태는 바뀌지 않는다.
이미 수행한 작업과 할당했던 scratch peak는 진단값에 남는다.

## 5. 회수의 정확한 수명 경계

Movement의 자동 수집은 owner 검사를 통과한 후, domain AND를 만들기 전에 다음을
한 번 확인한 경우에만 사용한다.

```rust
self.arena_owner != arena.owner_id() && arena.private_node_count() == 0
```

이는 prefix가 소유한 board와 비어 있는 새 worker suffix다. Fork 안에서 만든 board나
다른 private root를 이미 보유한 fork에서는 기존 수명 계약을 유지한다.

| 수집 경계 | 다음 단계에도 필요한 private roots |
| --- | --- |
| 각 collision-free pose 식 완성 후 | domain, free |
| 각 straight-down 증명 전이 후 | domain, free, reached |
| 각 일반 source-pop의 모든 전이 후 | domain, free, reached, pending |
| 모든 legal row가 완성되고 relation을 drop한 후 | 최종 legal row roots |

source-pop의 delta, slot별 remaining, candidate, novel은 그 경계를 지난 뒤 사용하지
않는다. Free 단계의 outside-domain complement는 domain과 같은 node를 공유하므로
domain marking으로 보존하고, 실제 이동 후 새 ID에서 다시 만든다. 최종 relation의
owner는 모든 이동 중 회수가 끝난 뒤 기록한다.

완성된 worker는 기존처럼 seal하여 생성용 unique/apply map을 해제한다. Finalization이
오류로 끝나도 공통 work flush를 지나 오류를 반환하므로 마지막 저빈도 pulse 뒤에
남은 GC work도 빠뜨리지 않는다. GC work는 총 Boolean work에 이미 포함되며 두 번
더하지 않는다. Piece별 관측은 독립 atomics와 별도 sampler를 사용한다.

## 6. 정확성 검증

| 검증 집합 | 실제 결과 | 주요 범위 |
| --- | --- | --- |
| Boolean | 14 passed | 보완 edge, 모든 유한 truth table, canonical 재생성, cofactor, GC 전후 cache 및 숫자 ID 재사용, owner/checkpoint, skip, NodeLimit, mark 중 취소, commit 전 WorkLimit |
| Symbolic family | 38 passed | 모든 builtin/imported ordered kicks, seeded/unseeded fixed point, 부분 domain, detour, 강제 source-pop remap 후 후속 이동, fork-local/preexisting-root 보호, 공급/Hold/EOF, archive |

새 42개 큐 fixture는 실제 standard 7-bag의 두 draw, 초기 I Hold, 6×2 빈 영역에 대한
세 번의 lock을 사용한다. 기존 경로의 해법 identity와 source coverage를 비교하고,
각 identity×원본 source bit의 대응까지 확인한다. 개수만 같고 source가 바뀌는 오류를
허용하지 않는다.

처음 작성한 32개 명시적 다중 큐 fixture는 연구 경로가 compact relation을 지원하지
않는 입력이어서 `Unsupported(source_has_no_compact_unique_queue_relation)`로
거절됐다. 이 제한을 우회하는 fallback을 추가하지 않고 지원되는 실제 bag fixture로
교체했다. 첫 실패 log와 수정 후 결과를 둘 다 보존했다.

원시 검증 기록은 `_local/artifacts/symbolic-dependency-tests-20261123.log`,
`symbolic-dependency-tests-r2-20261123.log`,
`symbolic-private-gc-boolean-tests-20261123.log`,
`symbolic-private-gc-family-tests-20261123.log`다. 첫 test build는 기존과 달리 debug
정보를 포함해 87초가 걸렸다. 이후 기존 `CARGO_PROFILE_TEST_DEBUG=0`을 복원했으며
수정된 첫 후보 build는 23.34초, GC test build는 52.85초였다. 서로 다른 debug/profile과
source의 시간을 알고리즘 성능으로 비교하지 않는다. 전체 workspace/Windows/CI 검증을
실행했다는 뜻도 아니다.

## 7. P7P4 실측

입력은 기존과 같은 empty 10×4, actual fresh standard 7-bag 11 draws(P7P4), 10 locks,
empty enabled Hold, Jstris180, FullQueueOracle, CountUnique, probability Omit,
Summary다. 4,233,600개 원래 큐의 identity를 유지하고 아카이브 전용 경로를 사용한다.
모든 사용 가능한 논리 CPU 9개를 선택하며, 유효한 미노별 작업은 7개다. 표현·work·시간
제한은 추가하지 않았고 candidate fallback이나 자동 resource 축소 재시도도 없다.

첫 후보의 실제 identity는 다음과 같다.

| 항목 | 값 |
| --- | --- |
| Artifact prefix | `_local/artifacts/full4-symbolic-ready-dependencies-20261123` |
| Compiled source tree | `6cd017819a608e70990ba775eb6a89bf8caccc09` |
| Binary SHA-256 | `e42f157c44285c5f4077db5b0d9dfbb8d74e411a7e31d8e2820a4769d39bd8e5` |
| Runtime receipt | `1795432994982663532-9` |
| Manager duration / exit | 18.129초 / signal 9, code 137 |
| Rank 2 완료 최초 관측 | 10.011775475초 |
| 마지막 표본 | 17.024986655초, rank 3 모두 Propagation |
| Child 표본 최대 RSS / VmHWM | 2,709,594,112 / 2,913,419,264 B |
| Manager process-group 표본 최대 | 2,374,356,992 B |
| Supervisor termination attempts | 없음 |

이전 `transfer-descriptor`와 request/source 객체 및 fingerprint가 같은 것을 독립적으로
대조했다. Rank 3 이동 중 다른 arena의 작업이 멈춰 있는 표본을 비교하면 다음과 같다.

| 누적 관측 | 이전 후보 | 공유 의존성 후보 |
| --- | ---: | ---: |
| Rank 2 완료 최초 표본, 초 | 42.055477172 | 10.011775475 |
| Bridge work pulse | 53,739,520 | 10,223,616 |
| Down work pulse | 329,252,864 | 92,798,976 |
| ZDD work pulse | 408,420,352 | 136,052,736 |

Pulse는 `2^18` 단위의 누적 관측이며 정확한 함수 호출 횟수는 아니다. 이는 ready-domain,
공유 semijoin, 이동 증명을 합친 후보의 관측이고 각각의 효과를 따로 식별한 실험은 아니다.
Rank 2 비용은 줄었으나 rank 3의 private nodes는 미노별 약 275만–456만까지 증가했다.
그 단계의 shared prefix는 985,742 logical nodes / 139,198,464 B였다. 전체 peak RSS나
전체 해법 수, 완주 시간의 개선을 이 중단된 실행으로 주장하지 않는다.

GC 후보의 실측은 다음과 같다. 두 후보와 이전 기준의 `compiled_request`, `source`
전체 JSON 객체 및 fingerprint를 대조해 동일한 입력임을 확인했다.

| 항목 | GC 후보 |
| --- | --- |
| Artifact prefix | `_local/artifacts/full4-symbolic-private-gc-20261123` |
| Compiled source tree | `fa0f6c84b4e821078e9cf0d594cfeb52023e2e7a` |
| Binary SHA-256 | `2307a81a9d42bdd4a8e8680599875802cf8b20472ebae0b719b18ad4be3732ba` |
| Runtime receipt | `1795433777287611454-9` |
| Manager duration / exit | 21.211초 / signal 9, code 137 |
| Rank 2 완료 최초 관측 | 9.00721956초 |
| 마지막 표본 | 20.051807091초, rank 3의 O는 Locks, 나머지 6종은 Propagation |
| Child 표본 최대 RSS / VmHWM | 2,803,404,800 / 2,812,477,440 B |
| Manager process-group 표본 최대 | 2,812,809,216 B |
| Supervisor termination attempts | 없음 |

마지막으로 관측한 각 rank/piece의 독립 카운터를 합하면 회수 55회,
12,606,518 nodes 회수, 28,832,513 apply 항목 폐기, 회수 work 346,151,506이다.
Rank 3만의 해당 값은 47회, 11,232,976 nodes, 24,535,856 apply 항목,
328,078,244 work다. 마지막 전체 Boolean work pulse는 704,244,626이며,
관측한 회수 work 합이 약 49.15%에 해당한다. **Work 단위별 실제 비용은 서로 다르며
이 비율은 CPU 시간 비율이 아니다.** 표본에서 놓친 말단 작업도 있을 수 있다.

최종 Bridge/Down work pulse는 첫 후보와 같은 10,223,616 / 92,798,976이며,
ZDD는 135,790,592다. 회수 후에도 rank 3은 완료되지 않았고 전체 identity/source
정확성 검증이나 CTK4 결과 저장 단계에 도달하지 않았다. 9.0초와 10.0초의 한 번씩의
표본 차이로 GC의 순수 속도 이득이나 전체 peak 감소를 주장하지 않는다. 살아 있는
이동 집합의 표현 비용을 줄이는 구조적 변경이 여전히 필요하다.

요약 추출은 Rust helper `symbolic-compact-observe-20261123.rs`로 수행했으며
`full4-symbolic-private-gc-20261123.compaction-observation.json`에 계산 근거를 보존했다.
이 후보의 검증된 소스는 로컬 checkpoint commit `833d589`에 보존했다.

## 8. 빌드와 저장 공간

첫 후보의 release `--timings`는 121.5초, 60 fresh / 2 dirty units를 기록했다.
Executor 88.98초(frontend 8.19초, codegen 80.79초), compare binary 32.29초다.
Thin LTO, codegen-units=1과 기존 incremental 설정을 바꾸지 않았다. 두 dirty unit의
의존 관계 때문에 max active concurrency가 1인 것과 jobs/CPU가 9인 것은 구별한다.

GC 후보 release는 135.7초, 역시 60 fresh / 2 dirty units다. Executor 103.33초
(frontend 20.58초, codegen 82.75초), compare binary 32.25초다. 실행 최적화 설정을
약화하지 않았으며, 다른 소스의 두 번의 build 차이를 cache 개선 효과로 해석하지 않는다.
HTML과 Rust 추출 기록은 `_local/artifacts/cargo-timings-private-gc-20261123.json`에
연결해 두었다.

빌드와 탐색의 `TMPDIR`만 허용된 overlay 경로 `_local/tmp`로 명시했다. 다른 프로젝트가
사용하는 `/tmp` tmpfs를 정리하거나 worker 수를 줄이지 않았다. 작업 디스크에는 첫
실행 양 경계에서 약 38.87GB가 남았고 `/tmp` 가용 공간도 동일했다. 경계의 여유 공간과
storage error 부재는 확인했지만, 외부 SIGKILL sender나 kernel OOM을 특정한 것은 아니다.

## 9. 남는 비용과 다음 구현

회수는 Boolean apply가 돌아온 안전 경계에서만 실행된다. 단일 AND/OR 내부의 peak,
살아 있는 family 자체의 폭, private node보다 apply cache 위주로 증가하는 경우는
별도 비용이다. Final legal roots가 줄어도 성장 trigger가 오지 않을 수 있고, 강제
final GC만 추가해도 Vec capacity가 유지되어 RSS 개선이 보장되지 않는다.

후속 연구 경로에는 원래 occupied cell 집합에서 ILC frame을 계산하는 movement
quotient와 `symbolic_cell_bridge.rs`의 exact image·eager fused filter를 구현했다.
`BooleanArena::cofactor_cells`는 여러 true/false cell을 한 traversal에서 완전히
제한하며, cell legality의 `C_r ∩ U = ∅`가 이미 선택한 row를 제외한다. 별도의
row-variable predicate를 cell predicate에 합칠 필요는 없다. 구현·유한 검증·별도
실측은 [`symbolic-cell-occupancy-2026-11-23.md`](symbolic-cell-occupancy-2026-11-23.md)에
기록하며, 이 문서 §7의 이전 실행 결과를 후속 구현의 결과로 해석하지 않는다.

이 변경은 movement의 변수 공간과 역필터를 바꾼 것이다. 원래 geometry family에
대한 Down 판정까지 점유만으로 대체한 것은 아니다. 점유와 미노별 배치 수량을 함께
쓰는 다음 geometry 설계에는 완전 producer의 더 강한 계약이 필요하다.
[`symbolic-occupancy-quotient-design-2026-11-23.md` §7](symbolic-occupancy-quotient-design-2026-11-23.md#7-후속-설계-점유와-미노-수량으로-geometry-연장-가능성-판정)에
임의 family의 반례, 적용 조건과 아직 구현하지 않은 연산을 구분했다.

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_cell_bridge.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_compatibility_filter.rs`, 구현 내용: 구현된 cell 역필터와 원래 geometry Down 판정의 실제 비용을 분리하고, 후속 geometry quotient를 검토할 때 완전 producer의 swap-closure 계약 및 원래 row/source relation 보존을 별도로 검증한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_boolean.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_parallel.rs`, 구현 내용: 실제 Published/Consuming 메모리가 남는 경우에만 final collection과 읽기 전용 node packing을 함께 검토하고, 추가 복사 peak·실패 원자성·실행 시간을 비교한다.
- [ ] 수정 파일: `crates/clearra-core-executor/examples/symbolic_family_compare.rs`, `docs/research/symbolic-dependency-compaction-2026-11-23.md`, 구현 내용: Full4가 완료된 후보에서 accepted CTK4와 전체 identity/source 결과를 기존 경로와 대조하고, 60초 미만 완주 조건을 만족한 뒤 Full6로 확장한다.

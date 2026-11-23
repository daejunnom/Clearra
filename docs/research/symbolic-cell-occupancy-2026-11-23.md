# 원래 셀 점유를 이용하는 symbolic movement 구현 — 2026-11-23

## 1. 구현 상태와 범위

**Exact cell image와 eager 역필터를 명시적 연구 실행 경로로 구현했다.** 원래
placement/source joint ZDD는 그대로 유지하고, movement predicate의 변수만 catalog
row ID에서 원래 좌표의 cell ID로 바꾼다. 공개 API와 연구 CLI에서 선택해야 하며,
기존 제품 dispatch나 기존 row-variable 연구 경로를 자동으로 교체하지 않는다.

확인한 로컬 검증은 Boolean 18개, cell bridge 9개, family 관련 43개로 **서로 다른
70개 테스트가 통과**했다. 이는 아래 유한 correctness 검증 범위이며 Full4 완료나
전체 입력의 성능 보장이 아니다. 첫 **중간 후보**의 release 빌드와 실제 P7P4 실행은
§8에 기록했다. 그 실행은 444.039초에 signal 9로 종료됐고 결과는 미완료다.
후속 piece-batch 후보의 구현과 실제 실행은 §9에 기록했다. 후속 후보도 285.243초에
signal 9로 종료되어 Full4는 미완료다. CI 실행·조회·대기는 하지 않았다.

설계 근거와 미구현 대안은
[`symbolic-occupancy-quotient-design-2026-11-23.md`](symbolic-occupancy-quotient-design-2026-11-23.md)에
남아 있다. 그 문서의 safe-superset domain과 lazy `(z,p,M)` 역필터까지 구현한 것은 아니다.

## 2. 바꾼 계산과 보존하는 identity

기존 큰 실행에서는 rank 3 movement의 Boolean 표현이 계속 증가했다. 서로 다른 row
분해가 같은 칸을 차지해도 row-variable 공간에서는 다른 assignment다. 셀 경로는
물리 계산에서 그 중복을 합칠 수 있도록 원래 점유를 다음처럼 정의한다.

\[
U(S)=I\cup\bigcup_{r\in S} C_r.
\]

I는 초기 보드, C_r은 catalog row의 원래 footprint다. 현재 표준 Board64 PC 계약에서
4L은 40개, 6L은 60개 cell 변수를 사용한다. 변수 수가 줄었다는 사실만으로 BDD 크기나
시간이 줄어든다고 보장하지 않는다.

| 표현 | 구현에서 맡는 역할 | 합치지 않는 정보 |
| --- | --- | --- |
| 원래 row/source ZDD | 선택한 placement와 원래 queue의 joint relation | CountUnique identity, source, Hold/EOF 상태 |
| 원래 점유 U의 cell-BDD | 물리 보드 domain과 movement 조건 | 서로 다른 row 분해는 역필터의 원래 ZDD에 남음 |
| 삭제 frame D와 물리 B | U에서 기존 줄 삭제·압축 DP로 계산 | 같은 B라도 다른 D의 catalog row 답은 구별 |
| 원래 geometry family | 부분 선택의 extendibility 검사 | 점유가 같다는 이유로 Down(F)를 생략하지 않음 |

[`SymbolicOccupancy::compile_cells`](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_symbolic_family.rs)는
primitive cell 변수로 같은 U→D/B 계산을 사용한다. Cell mode의 row legality는 target
footprint의 모든 원래 칸이 비어 있어야 한다는 조건도 검사한다. 원래 row가 이미
선택됐다면 그 footprint가 U에 포함되므로 다시 추가할 수 없다. 기존 selected/add/remove
semijoin의 일대일 조건을 유지하면서 ILC realization은 D별로 판정한다.

미노 종류·queue 이름·profile별 특수 분기를 추가하지 않았다. 기존 verified ordered
kick/spawn authority와 transfer topology를 사용한다. Boolean/CountUnique 연구 계약을
넘는 점수·스핀 trace·입력 비용 목적함수의 지원을 새로 주장하지 않는다.

## 3. Exact image: 보드 목록을 만들지 않는 node별 계산

[`CellFamilyBridge::domain`](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_cell_bridge.rs)은
row/source ZDD의 공유 node를 방문해 다음 characteristic function을 만든다.

\[
X_R=\{U(S):\exists q,\ (S,q)\in R\}.
\]

EMPTY의 image는 FALSE다. UNIT이나 비어 있지 않은 source-only suffix의 image는 초기
보드 I 하나를 나타내는 정확한 cube다. row r의 node에서는 다음 식을 적용한다.

\[
\operatorname{img}(z)=\operatorname{img}(low)
 \lor \operatorname{force}_{C_r}(\operatorname{img}(high)),\qquad
\operatorname{force}_{C}(f)=(\exists C\,f)\land\operatorname{cube}(C=1).
\]

`force_cells_true`는 complete restriction과 다르다. 원래 셀의 두 값을 image에서
합친 뒤 새 점유를 true로 고정하므로, footprint가 겹치는 일반 OR image도 정확하다.
Boolean complement를 존재 양화 밖으로 잘못 꺼내지 않는다.

Image memo는 호출 내 ZDD node별이며 완성 row set이나 board assignment를 열거하지
않는다. 각 node의 image를 한 번 공유하더라도 Boolean force/OR 연산 자체의 비용은
입력 함수 크기에 따라 달라진다. 전체 변환이 원본 DAG 크기에 선형이라고 주장하지 않는다.

## 4. Eager 역필터와 source 상관

`CellFamilyBridge::filter`는 다음 원래 관계의 부분집합을 반환한다.

\[
\{(S,q)\in R:P(U(S))=1\}.
\]

초기 셀을 true로 제한한 뒤, 어느 suffix도 제공하지 않는 cell을 false로 제한한다.
지원 집합 T(z)는 해당 ZDD node 아래의 모든 가능한 row footprint의 union이다.
row r의 low/high child가 l/h일 때 residual은 다음과 같다.

\[
\begin{aligned}
p_l&=p[T(z)\setminus T(l)\leftarrow0],\\
p_h&=p[C_r\leftarrow1]
       [T(z)\setminus(C_r\cup T(h))\leftarrow0].
\end{aligned}
\]

High의 true/false mask는 서로소이며 `cofactor_cells`가 한 번에 완전하게 제한한다.
이미 true였던 셀은 residual 전체에서 제거되어 후속 false 제한이 그 사실을 덮어쓰지
않는다. Low에서 다른 row가 제공할 수 있는 셀은 false로 만들지 않는다.

따라서 product memo는 `(ZddId, 전체 signed BooleanId)`다. 별도 pending-cell mask나
materialized row-BDD pullback을 만들지 않는다. Source 경계에서 참이면 **원래 suffix**를
반환한다. 서로 다른 queue나 Hold 상태의 부분 결과를 독립적으로 조합하지 않는다.
선택한 row와 source의 전체 상관은 결과 ZDD에 남는다.

## 5. 재사용·소유권·메모리 수명

| 계산·저장소 | 현재 수명과 재사용 규칙 |
| --- | --- |
| Suffix support T(z) | ZDD `Weak<()>` owner와 epoch에 묶인 node cache. 같은 owner의 append-only node는 재사용 |
| ZDD compact 경계 | Rank 끝에서 support cache를 해제. 다른 owner/epoch를 만나도 자동 무효화 |
| Image·residual memo | 호출 내에서만 존재. Boolean rewind 뒤 숫자 ID를 다시 사용하지 않음 |
| Movement shared prefix | 기존 immutable prefix를 fork들이 공유. Private roots의 owner 경계 유지 |
| Row별 residual | 기존 legal roots 뒤 checkpoint를 잡고, 역필터 완료 또는 거절 뒤 rewind |

이 재사용으로 같은 row-family의 지원 집합을 수백 개 legal row마다 다시 전수 방문하는
일을 피한다. Cache는 `max_memo`를 지키며 work와 node 수를 별도 보고한다. Weak token은
ZDD graph의 소유 수명을 늘리지 않는다.

기존 row bridge는 predicate를 읽기만 하므로 worker가 `seal_for_reading()`을 사용한다.
Cell bridge는 새 cofactor node가 필요하므로
[`symbolic_family_parallel.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family_parallel.rs)의
cell worker는 봉인하지 않고 private apply cache만 먼저 해제한다. Unique table과 legal
roots는 consumer가 사용하는 동안 남는다. Row별 rewind는 임시 residual을 제거하며
원래 legal roots와 반환된 ZDD 결과는 유지한다. 별도 base 복제는 하지 않는다.

Consumer가 추가한 Boolean 작업도 실제 비용이다. 성공·실패 모두 consumer 처리 뒤
work를 flush하고 `record_movement`에 합산한다. 그 뒤 worker suffix를 drop한다.
Cofactor 도중 implicit GC나 owner 변경을 하지 않으며, 명시적 자원 거절·취소는 EMPTY와
구별한다. 0인 명시 한도는 기존처럼 unlimited이고 시간 제한이나 후보 fallback을 넣지 않았다.

## 6. API·CLI·계측 provenance

공개 연구 API는
[`solve_symbolic_family_with_cell_occupancy(problem, limits, control)`](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family.rs)다.
기존 `solve_symbolic_family`는 row-variable 경로를 유지한다. 두 API의 원래 result와
source/identity 검증 계약은 같다. `symbolic-family` feature가 제품 dispatch를 바꾸지는 않는다.

- [`symbolic_family`](../../crates/clearra-core-executor/examples/symbolic_family.rs): `--cell-occupancy`로 선택한다.
- [`symbolic_family_compare`](../../crates/clearra-core-executor/examples/symbolic_family_compare.rs): `--backend symbolic-cells`로 선택한다.

비교 하네스의 `--archive-only`는 CTK4를 저장하고 identity/source 열거 export를 생략한다.
Solver의 집합 연산에 완성 후보 iterator를 넣지 않았다는 사실과, 선택한 출력 export가
결과를 순회할 수 있다는 사실을 구분한다. 전체 identity·global source 집합 비교도
full joint relation 검증을 자동으로 대신하지 않는다.

보고서는 `movement_variable_space`를 `original-cells` 또는 `catalog-rows`로 명시한다.
후속 piece-batch 후보는 `geometry_filter_scope=piece-union`을 추가하며, 기존 row
경로는 `row-extension`으로 기록한다. `geometry_filters`는 이 단위의 실제 호출 수다.
Rank별 `cell_support_work_steps`, `cell_support_cached_nodes`와 progress의
`symbolic_cell_support_work`, `symbolic_cell_support_entries`를 추가했다. Support work는
이미 `bridge_work_steps`에 포함되므로 합산 때 두 번 더하지 않는다. Work·retained payload·
RSS는 서로 다른 지표이며 node 수만으로 메모리 peak를 환산하지 않는다.

비교 자료에는 request/source fingerprint와 compiled source tree·binary fingerprint,
worker 정책·실행 receipt를 함께 남긴다. Cell mode도 원래 queue identity를 바꾸지 않는다.
실행 전후 환경이 바뀐 사실은
[`2026-11-23-storage-process-pressure.md` §10](2026-11-23-storage-process-pressure.md)에 기록했다.
가용 RAM 변화가 있으므로 과거 실행 대비 차이를 전부 알고리즘 효과로 귀속하지 않는다.

## 7. 확인한 로컬 검증

| 선택 suite | 결과·test 실행 시간 | 원시 로그와 receipt |
| --- | --- | --- |
| Boolean | 18 PASS, 0.05초 | [symbolic-cell-boolean-tests-20261123.log](../../_local/artifacts/symbolic-cell-boolean-tests-20261123.log), `1795434872629091166-19` |
| Cell bridge | 9 PASS, 0.03초 | [symbolic-cell-bridge-tests-20261123.log](../../_local/artifacts/symbolic-cell-bridge-tests-20261123.log), `1795435003768980025-3` |
| Family·movement·supply 등 | 43 PASS, 1.59초 | [symbolic-cell-family-tests-r2-20261123.log](../../_local/artifacts/symbolic-cell-family-tests-r2-20261123.log), `1795435018542906411-4` |

Boolean의 최초 test-profile 빌드는 43.31초, 보정한 family suite의 증분 빌드는 16.07초였다.
위 test 시간은 solver benchmark 시간이 아니다. 이 보고서는 추가 테스트를 실행하지 않고
완료 로그를 읽어 작성했다.

Bridge fixture는 3-row의 모든 family에 대한 OR image, 3-cell의 모든 Boolean predicate에
대한 원래 identity×source 답, 초기 점유·겹침·zero suppression·bit 63·signed predicate를
확인한다. 같은 숫자 ID를 가진 다른 arena, compact 후 epoch 변화, support 재사용과
명시적 해제, 한도·취소가 EMPTY와 구분되는 경우도 포함한다.

Family suite는 cell mode의 builtin kick별 source pair 일치, parallel owner/cofactor 수명,
동일한 물리 B에 서로 다른 D가 필요한 4×3 ILC 반례를 포함한다. 기존 imported ordered
table 구별, Hold/EOF/source 상관, archive 회귀도 같은 선택 suite에서 통과했다.

첫 family 로그의 42 PASS/1 FAIL도
[symbolic-cell-family-tests-20261123.log](../../_local/artifacts/symbolic-cell-family-tests-20261123.log)에
보존했다. 그 fixture는 두 terminal보다 작은 `max_predicate_nodes=1`에 Resource를
기대했지만 실제 계약은 invalid input이었다. 유효한 최소 2로 resource 거절 fixture를
보정한 뒤 43개가 통과했다. 첫 실패 기록을 성공한 실행으로 바꾸지 않는다.

최종 정적 확인에서 이번 변경의 Rust 15개 파일은 child module 자동 순회를 제외한
`rustfmt --check --config skip_children=true`를 통과했다. Module root를 통해 기존
하위 파일까지 순회한 별도 확인에서는 변경하지 않은 세 파일의 기존 서식 차이가
나왔다. 이 결과를 전체 저장소 formatter 통과로 바꾸어 표현하지 않으며, 범위를 넓힌
서식 수정도 하지 않았다. 변경분의 `git diff --check`는 통과했다.

## 8. 첫 중간 후보의 release와 P7P4 관측

### 측정 대상과 build

측정한 compiled source tree는 `791cb54f5bded0c0224e7e8209f3acbe103a10e4`, binary SHA-256은
`8a17d4256941b0bcc6331aafc5bbf5d276a68b449be3c6b8e9525abbda342acf`다. Source commit 표시는
기반 checkpoint `833d5893cd84679e3457a9f8310b7e966cd954e2`이므로, 실행 source 식별에는
실제 compiled tree와 binary 지문을 함께 사용한다. 이후 piece 처리 구조의 후보와
같은 측정으로 합치지 않는다.

Release는 저장소의 ThinLTO·codegen-units=1 설정을 유지했다. Cargo timings는
**135.9초**, 60 fresh·3 dirty units, 최대 동시 unit 2개(jobs=9, ncpu=9)를 보고했다.
기존 cache가 있는 source 재빌드이며 clean build나 같은 source의 warm 비교가 아니다.

| 재빌드 unit | 시작 초 | 소요 초 | 해석 |
| --- | ---: | ---: | --- |
| `clearra-core-executor` | 0.14 | 102.32 | Frontend 18.85초, codegen 83.47초 |
| `symbolic_family_compare` | 102.46 | 33.44 | Executor 뒤 critical path |
| `symbolic_family` | 102.46 | 13.72 | Compare binary와 동시에 진행 |

두 binary의 시간을 직렬로 더하지 않는다. Build receipt는 `1795435097108658560-10`,
근거는 [symbolic-cell-build-20261123.log](../../_local/artifacts/symbolic-cell-build-20261123.log)와
[cargo-timings-cell-20261123.json](../../_local/artifacts/cargo-timings-cell-20261123.json)이다.

### 실행 조건과 완료 범위

Prefix는 `_local/artifacts/full4-symbolic-cell-occupancy-20261123`, 실행 receipt는
`1795435254914677505-9`다. `movement_variable_space=original-cells`, archive-only 실행이며
empty 4L / P7P4 / Jstris180 / 10 locks·11 draws / Hold 허용·초기 Hold 없음 /
FullQueueOracle / CountUnique / original source 4,233,600개 조건이다. Worker 9·모든
논리 프로세서 사용 정책과 max_seconds=0을 유지했다.

직전 private-GC 실행과 `compiled_request`·`source` JSON block을 직접 대조해 차이가
없었다. Request fingerprint는 `e709af430f47ba7292eba224b3af7414efaa13560082dc77c474089992afa924`,
source fingerprint는 `989d0aa152b2a75913d02142fcca6df8d6e88302de116c84060fb22decc81d56`이다.
다만 실행 전 MemAvailable은 8,765,673,472 B로, 이전 후보의 시작 환경과 달랐다.

프로세스는 **444.039초에 raw signal 9 / 137**로 종료됐다. 탐색·identity export·global
coverage·joint comparison은 완료되지 않았다. Rank 3의 첫 progress 관측은
10.007195024초다. 별도 rank-completion marker map은 비어 있으므로 새로운 완료 시각이나
완료 성능을 추정하지 않는다. Solver 결과가 없으므로 해법 수나 source coverage를
확정하지 않는다.

### 마지막 진행과 메모리 구조

마지막 442.857241341초 표본은 rank 3 / T / catalog row ID 253 / `row_down`이다.
442개 coordinator phase 표본 중 396개가 `row_down`, 20개가 `row_bridge`, 25개가
`row_merge`, 1개가 `movement`였다. 이는 coarse sample 수이며 정확한 단계 CPU 시간이나
동시에 돌아간 모든 worker의 시간 분배가 아니다.

| 마지막 표본의 항목 | 관측값 |
| --- | ---: |
| Boolean cumulative work | 350,093,418 |
| Bridge cumulative work | 4,718,592 |
| Support work, 위 bridge에 포함 | 59,642 |
| Down cumulative work | 2,091,122,688 |
| ZDD cumulative work | 2,640,052,224 |
| Support cache entries | 19,331 |
| Joint ZDD nodes / retained payload 추정 B | 62,853,083 / 4,904,567,168 |
| Compatibility memo entries / retained payload 추정 B | 111,241,849 / 1,526,732,768 |
| Shared Boolean prefix nodes / retained payload 추정 B | 506,125 / 33,816,576 |

이 관측은 다음 구조 검토에서 compatibility/Down과 joint ZDD 증가를 우선 확인할 근거다.
마지막 retained payload를 실제 동시 peak처럼 더하지 않는다. Manager process-group RSS
표본 peak는 **7,336,996,864 B**, 자식 표본의 최대 RSS와 VmHWM은 모두
**7,334,789,120 B**였다. 과거 후보와 종료까지 걸린 시간이나 RSS만 비교해 성능 향상을
계산하지 않는다. 진행 단계와 환경이 모두 달랐고 어느 실행도 Full4를 완료하지 못했다.

Manager 신호 시도와 stderr 종료 event는 없고 forced reason=null, timeout=0,
pressure events=0, automatic retry=false였다. 저장 공간·inode는 실행 양쪽 경계에서
고갈되지 않았다. 보존된 `/proc/vmstat` 표본은 실행 중 `oom_kill=12`, 종료 뒤 13을
보인다. 이는 새 커널 OOM-kill event 증거지만 victim PID는 특정하지 않는다. 시각·
전역 범위·cgroup 증거의 한계와 upstream source 근거는
[환경 보고서 §11](2026-11-23-storage-process-pressure.md)에 기록했다.

원시 자료는 같은 prefix의 `.summary.json`, `.observation.json`, `.movement-observation.json`,
`.compaction-observation.json`, `.runtime.json`, `.before.storage.json`, `.after.storage.json`,
`.vmstat-during.txt`다. 이 절은 첫 중간 후보의 결과로 보존한다.

## 9. 미노별 기하 검사 묶음 — 후속 후보

### 정확한 분배와 source 복원

첫 cell-space 후보에서 movement를 통과한 각 row extension에 Down(F)를 반복 적용하는
비용이 컸다. 후속 후보는 **cell opt-in 경로**에서 한 미노의 물리적으로 허용된 extension을
먼저 합치고, 기하 predicate를 한 번 적용한다. 기존 row-variable 경로는 per-row 검사다.
Row r의 이전 선택 집합을 S_r, row를 추가한 집합을 E_r이라 하면 다음과 같다.

\[
E_r=\operatorname{add}_r(S_r),\qquad
E=\bigcup_r E_r,\qquad
A=E\cap\operatorname{Down}(F),\qquad
A_r=A\cap E_r=E_r\cap\operatorname{Down}(F).
\]

마지막 등식은 E_r⊆E에 따른 분배법칙이다. 임의의 geometry family F에서도 성립하며,
F가 모든 완전 덮개를 포함한다는 추가 가정을 요구하지 않는다. A=E이면 row별 교집합을
생략하고, A_r=E_r이면 기존 S_r을 재사용한다. 나머지는 row r을 제거한 뒤 **원래
상태별 row/source joint relation**과 prefix semijoin한다. 서로 겹치는 E_r이 있더라도
마지막 row와 원래 source의 상관을 독립적인 합집합으로 대체하지 않는다.

완성 보드·queue의 목록을 생성하지 않는다. `(row_id, selected, extended)`의 짧은
벡터에는 해당 미노의 legal catalog row 수만큼 ZDD root를 보관한다. 이 root들이 살아
있는 동안 ZDD를 compact하지 않는다. Compatibility memo는 aggregate Down 완료 직후,
row별 교집합과 source 복원 **이전**에 해제한다. 받아들인 root는 ZDD arena에 남아
있으므로 memo 해제 때문에 해법이 사라지지 않는다.

검사 호출 수 감소가 실행 시간·메모리 감소를 보장하지는 않는다. E의 합집합과 A_r의
교집합에서 생기는 ZDD 중간 node는 rank compact까지 남으며, aggregate Down 자체도
큰 product graph가 될 수 있다. 이 비용을 실제 실행으로 구별하기 위해 `piece_union`,
`piece_down`, `row_intersect` phase와 `geometry_filter_scope`를 기록한다. Archive phase는
늘어난 phase 배열의 마지막 원소가 아니라 이름으로 찾도록 함께 수정했다.

### 검증과 release provenance

변경 영향을 받는 family suite **43개가 다시 통과**했다. Test 실행 1.62초, test-profile
재빌드 23.60초이며, 로그는
[symbolic-cell-piece-batch-tests-20261123.log](../../_local/artifacts/symbolic-cell-piece-batch-tests-20261123.log),
receipt는 `1795436027920877478-13`이다. Boolean 18개와 bridge 9개의 구현은 직전
검증 이후 바뀌지 않았다. 반복 실행한 43개를 서로 다른 70개에 다시 더하지 않는다.

측정한 source tree는 `54561f9a5bbb0b3067f09c9b2dd60f686550f246`, 기반 source commit은
`4ff8bd590d671d5b8fe249378998eef9f018d387`, compare binary SHA-256은
`2866d1436de712460bc46a805a04c72cc4b540fe269da4da5f73840482f8de2e`다.
Release의 ThinLTO·codegen-units=1·기존 target cache를 유지했으며 Cargo timings는
**138.3초**, 60 fresh·3 dirty units, 최대 동시 unit 2개(jobs=9, ncpu=9)를 기록했다.

| 재빌드 unit | 시작 초 | 소요 초 | 관측 |
| --- | ---: | ---: | --- |
| `clearra-core-executor` | 0.13 | 99.61 | Frontend 7.40초, codegen 92.21초 |
| `symbolic_family_compare` | 99.74 | 38.58 | Executor 이후 critical path |
| `symbolic_family` | 99.74 | 14.55 | Compare와 동시 실행 |

Build receipt는 `1795436126150741896-10`, 근거는
[symbolic-cell-piece-batch-build-20261123.log](../../_local/artifacts/symbolic-cell-piece-batch-build-20261123.log)와
[cargo-timings-cell-piece-batch-20261123.json](../../_local/artifacts/cargo-timings-cell-piece-batch-20261123.json)이다.
소스와 cache 상태가 다른 재빌드이므로 두 후보의 build 시간 차이를 최적화 효과로
단정하지 않는다. 기존 warning이 남아 있으며 warning-free 빌드로 표현하지 않는다.

### P7P4 실제 실행과 첫 후보의 대조

Prefix는 `_local/artifacts/full4-symbolic-cell-piece-batch-20261123`, 실행 receipt는
`1795436508607349449-11`이다. 원래 source 4,233,600개·empty 4L·Jstris180·10 locks·
11 draws·초기 Hold 없음·FullQueueOracle·CountUnique·archive-only 조건을 유지했다.
9개 논리 CPU를 사용하는 worker 정책, `--no-timeout`, 0인 명시적 work/node 한도와
implicit deadline 없음도 유지했다. 새 구조 후보의 한 번 실행이며 실패 후 worker나
메모리 계약을 바꿔 자동 재시도하지 않았다.

Rust 추출기로 두 실행의 `compiled_request`와 `source` JSON block 전체를 비교한 결과
**둘 다 정확히 같았다**. Request/source fingerprint도 §8과 각각 동일하다. 비교 자료는
[symbolic-cell-candidate-comparison-20261123.json](../../_local/artifacts/symbolic-cell-candidate-comparison-20261123.json)에
있다. 같은 요청이라는 사실과 같은 작업을 끝낸 성능 비교라는 주장은 구별한다.

| 관측 | Cell image + row별 Down | Cell image + piece별 Down |
| --- | ---: | ---: |
| 프로세스 종료까지 경과 초 | 444.039 | 285.243 |
| 종료 | Signal 9, 미완료 | Signal 9, 미완료 |
| Rank 3 첫 관측 초 | 10.007195024 | 7.01166183 |
| 마지막 표본 초 | 442.857241341 | 284.314960197 |
| 마지막 단계 | Rank 3 / T / row 253 / `row_down` | Rank 3 / T / `piece_down` |
| 다른 미노의 마지막 상태 | I/O/Z/J/L released, S published | I/O/S/Z/J/L released |
| 마지막 Boolean work | 350,093,418 | 387,605,754 |
| 마지막 Bridge work | 4,718,592 | 5,242,880 |
| 마지막 Down work | 2,091,122,688 | 1,480,327,168 |
| 마지막 ZDD work | 2,640,052,224 | 2,004,615,168 |
| Joint ZDD nodes | 62,853,083 | 95,369,928 |
| Joint ZDD retained payload 추정 B | 4,904,567,168 | 4,904,567,168 |
| Compatibility memo entries | 111,241,849 | 69,026,526 |
| Compatibility retained payload 추정 B | 1,526,732,768 | 954,210,272 |
| 자식 표본 최대 RSS / VmHWM B | 7,334,789,120 | 7,306,452,992 |
| Manager process-group RSS 표본 peak B | 7,336,996,864 | 6,443,675,648 |

종료까지의 285.243초를 성공 시간으로 사용하거나 두 시간을 나눠 speedup을 계산하지
않는다. 처리 순서와 마지막 진행 범위가 다르며 두 실행 모두 rank 3을 완료하지
못했다. 후속 후보에서 더 많은 piece가 released 상태였더라도 같은 업무량의
throughput, 최종 identity/source 일치 또는 전체 메모리 절약을 증명하지 않는다.
Manager와 자식의 peak 차이는 다른 표본 주기의 관측이며 이를 서로 교체하지 않는다.

후속 후보의 coordinator 표본 284개 중 `piece_down`은 240개였다. 나머지는 geometry 1,
movement 1, piece_union 2, rank_domain 1, row_add 2, row_bridge 20, row_merge 17개다.
`row_intersect`를 표본에서 보지 못했다고 비용이 0이라고 해석하지 않는다. 같은 원리로
이 표본 비율을 정확한 CPU 시간 분해로 쓰지 않는다. 마지막 support work는 62,889로
Bridge work에 이미 포함되며, support cache는 20,413 entries였다.

관측된 Boolean GC는 두 cell 후보 모두 5회, 회수 842,555 nodes, GC work 17,799,353,
폐기 apply entries 5,106,153이었다. 같은 GC 수가 두 전체 실행의 같은 상태 공간을
뜻하지는 않는다. Cell movement 이후의 **Down product와 중간 joint ZDD 증가**가
이번 결과에서 더 큰 구조 문제다. 단순히 더 많은 worker를 만들거나 같은 full 실행을
반복하는 것으로 해결됐다고 보지 않는다.

후속 실행의 커널 `oom_kill` 카운터는 **13→14**였고 manager 종료 시도·stderr signal
event는 없었다. Forced reason=null, timeout=0, memory-pressure events=0,
automatic_retry=false다. 종료 구간에 OOM-kill 사건이 있었음을 뒷받침하지만 victim
PID나 system/memcg 경로는 특정하지 않는다. 저장 공간과 inode는 두 경계에서 고갈되지
않았다. 자세한 환경 증거는 [환경 보고서 §12](2026-11-23-storage-process-pressure.md)에 있다.

Rank completion marker map은 이 실행도 비어 있다. 완성 solution CTK4, identity와
global original-source export, full joint comparison은 확보되지 않았다. **P7P4 전체
60초 이내 성공 조건이 성립하지 않아 Full6는 실행하지 않았다.** 작은 입력을 별도
성능 benchmark로 실행하거나 알려진 확률 명제를 다시 계산하지 않았다.

이 후보는 정확성 검증과 비용 관측이 가능한 명시적 연구 경로로 보존한다. 기존 제품
기본 경로의 교체나 완료 성능 우위 판정은 하지 않는다. 두 실행의 원시 로그·receipt·
경계·각 observation은 각각의 prefix로 보존하며 실패를 성공한 결과로 덮어쓰지 않는다.

## 10. 남는 한계와 다음 검증

Exact image, residual BDD, `(z,p)` product memo와 최종 joint ZDD는 여전히 커질 수 있다.
Canonicalization은 동등한 상태를 재사용하지만 모든 입력에서 작은 표현을 보장하지 않는다.
이번 구조 변경은 movement의 변수와 재사용 경계를 바꾼 것이며 Full4·Full6 완료나
점근적 개선을 증명한 결과가 아니다. §8·§9 실행은 모두 미완료다. Piece batching으로도
중간 ZDD 보관과 Down product 문제가 남았으므로 다음 우선순위는
[설계 보고서 §7](symbolic-occupancy-quotient-design-2026-11-23.md)의 조건부 `G(U,M)`
기하 oracle이다. 전체 catalog/target-multiset 완전성 authority를 먼저 확보해야 하며,
임의의 geometry 부분집합에 같은 축약을 적용하면 해법을 잘못 허용할 수 있다.
새 oracle이 작게 표현되거나 원래 joint ZDD 증가까지 해결한다고 미리 가정하지 않는다.

### P0 — 실제 실행 근거

- [ ] 수정 파일: `crates/clearra-core-executor/examples/symbolic_family_compare.rs`, 구현 내용: 완료 결과가 확보된 비교에서 원래 identity/source 계약과 full joint 검증 범위를 분리해 확인하고 자료에 기록한다.

### P1 — 후속 구조 선택

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_cell_bridge.rs`, 구현 내용: 실제 병목 근거가 있으면 exact image·eager cofactor·support cache의 비용을 나눠 검토한다. Safe-superset 또는 lazy 대안은 별도 정확성·수명 검증을 통과한 경우에만 구현한다.

### P2 — 큰 입력 채택

- [ ] 수정 파일: `docs/research/symbolic-cell-occupancy-2026-11-23.md`, 구현 내용: Full4의 완료와 결과 검산 없이 Full6의 속도·메모리 개선을 선언하지 않고, 다음 채택 근거와 미해결 항목을 기록한다.

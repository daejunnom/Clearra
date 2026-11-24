# GUI 옵션 일반화, P7P4 병목 및 메모리 수명 검증

작성일: 2026-11-24 UTC

## 1. 결과와 적용 결정

고정 필드 PC, Build, Recovery, Setup의 GUI 옵션이 실제 실행 요청과 결과 소비자로
이어지는 경로를 검토하고, B2B 의미에 따른 조기 가지치기, Finesse 계산 재사용,
공급 전이 재사용, 지연 재생 결과의 수명 및 GUI 진행 이벤트 보유 방식을 개선했다.
베타 사전 분석 버튼과 사용하지 않는 GUI GPU 진입점을 정리했고, Recovery의
중간 필드를 한 번만 줄일 수 있던 오류를 수정했다.

**최소 지지 수 피벗의 조기 종료는 정확성 검증을 통과했지만 제품 기본값으로
채택하지 않는다.** 동일 소스·바이너리·9 worker의 P7P4 비계측 실행은 기준 평균
147.996초, 조기 종료 평균 152.880초였다. 관측 평균은 3.300% 느렸으며, 각 두 번의
작은 표본과 중간 중단/대체 실행을 포함하므로 일반적인 성능 열세를 확정할 수는
없다. 채택할 개선 근거가 없다는 판단에 따라 기본값을 전체 검사로 되돌리고,
명시적인 비교 실험과 그 정확성 테스트는 유지한다.

메모리 병목은 후보 그래프 투영 자체보다 **병렬 검증 worker별 표준 bag 커버리지
자료와 미노 순서 언어**에 있었다. 기준 진단의 관측 RSS 피크는 4.679 GB이고,
그 시점에 연결된 worker 공개 용량에서 bag 자료가 3.448 GB, 순서 언어가
0.531 GB였다. 현재도 사용하는 캐시를 해제된 자료로 오인하여 제거하지 않았다.

이번 수치는 최종 변경 전체와 과거 릴리스의 속도 비교가 아니다. 조기 종료 한 축만
격리한 비교이며, B2B·Finesse·공급·GUI 수명 개선 각각의 전체 제품 speedup을
이 값에서 추론하지 않는다. 모든 GUI 옵션 범주를 검토했지만 가능한 모든 보드와
공급 및 옵션의 Cartesian 곱을 실행한 것은 아니다.

### 소스와 실행 범위

- 확보한 원격 기준: `8fa2dfc6ee195206f50a241d9888125ffaa67dbe`.
- 이전 소스 초안: `b75a1cc708389e50cd069ba5cbe647d351838664`.
- 완성된 변경 및 P7P4 고정 측정 소스:
  `49fd03da6255c024f71c92d21b305fe34ec99b94`.
- 측정 이후의 제품 변경은 위 조기 종료 기본값 복원과 진단 라벨·회귀 보정이다.
- 작업 브랜치: `codex/option-semantic-pruning-20261124`.
- Full 6L은 보류했다. family DAG 직접 풀이는 도입하지 않았다.
- CI 변경·실행·조회, 원격 push, 배포와 main 승격은 수행하지 않았다.

앞선 [소스 초안 보고서](option-semantic-pruning-2026-11-24.md)는 실행 owner가
해결되기 전의 상태를 보존한다. 그 문서의 미실행 상태를 이번 결과로 소급해 바꾸지
않는다. 세부 옵션 표는 [GUI 옵션 적용 범위](gui-option-coverage-2026-11-24.md),
재생 수명과 계수 권한은 [App 재생 수명 검증](app-replay-lifetimes-2026-11-24.md)에
정리한다.

## 2. 요청 의미에 연결한 계산 생략

### B2B 보존: 불가능한 clear를 이동 검사 전에 제외

`BackToBackPreservationPolicy`는 piece, 정확한 현재 lock의 줄 삭제 수, 현재 lock의
PC 여부로 `Preserves`, `Rejects`, `RequiresSpinEvidence`를 구분한다. 0줄·4줄·해당
lock의 PC 예외를 유지하고, spin 증거가 필요한 경우에는 기존 ordered kick과
최종 회전·코너/immobile 판정을 그대로 수행한다. 목표가 PC라는 이유로 중간의
일반 clear까지 PC 예외로 취급하지 않는다.

Compact BuildUp의 조건부 scoring graph와 extended 일반 PC 경로에서 확실한
`Rejects`를 이동 검사 앞에 둔다. 같은 operation의 모든 허용 pose가 동일한
child projection 및 원래 삭제 행 집합과 일치해야 한다는 사실을 이용한다.
공유 물리 도달성 캐시에 옵션 의존적인 거절을 기록하지 않는다. 조건 없는 원래
커버리지와 확률 분모를 만드는 경로에는 B2B 제약을 섞지 않는다.

Compact BuildUp의 B2B·Finesse 가지치기와 안정적인 간선 압축은 앞선 `b75` 초안
구현을 유지하고 이번에 실행 검증했다. 이번 추가 구현은 extended 일반 PC의
조기 거부와 Oracle의 큐별 최솟값·고정 큐 witness 재사용이다.

검증은 기존 완성 그래프에 나중에 필터를 적용한 결과와 조기 가지치기를 비교한다.
Extended 회귀에는 작은 fixture 6개 × Core 규칙 5개 × spin 프로필 6개가 들어간다.
일반 I clear 거절 예제는 불필요한 도달성 상태 방문이 0이 되는 것도 확인한다.
Core의 NoKick 검증은 현재 GUI에서 NoKick을 선택할 수 있다는 뜻이 아니다.

구현: [B2B 정책](../../crates/clearra-scoring/src/b2b_preservation.rs),
[compact BuildUp](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs),
[extended BuildUp](../../crates/clearra-core-executor/src/backend/wasm_cpu/extended_buildup.rs).

### Finesse: 이미 계산한 큐별 최솟값과 원래 간선 순서 재사용

완주할 수 없는 suffix와 확실히 거절된 target에는 Finesse evidence·target group·
movement BFS를 만들지 않는다. 원래 graph의 간선 ordinal은 유지하여 비용 동률의
witness 우선순위가 바뀌지 않게 한다. 간선 압축은 기존 버퍼를 재사용하고, 실제
살아 있는 간선 수를 기준으로 후속 자료 용량을 계산한다.

Oracle 전체 요약에서는 각 solution에 대해 이미 얻은 **큐별 최소 비용**을 합쳐
큐별 최솟값을 구한다. 전체 solution 언어의 추가 union과 같은 비용 DP를 다시
실행하지 않는다. 이는 solution별 평균의 최솟값을 구하는 방법이 아니다. 기존
전체 평균은 성공한 고유 큐들의 비용을 산술 평균하는 계약이며, 입력 가중치는
성공 확률 질량 계산에 계속 사용한다. Visible 7/Both는 숨은 입력을 관찰하기 전에
여러 대안을 함께 결정해야 하므로 필요한 joint language를 유지한다.

고정 큐의 대표 witness도 이미 선택한 결과를 재사용한다. 합성 언어의 메모리
상한은 prefix 수와 subset 수의 유효한 상한을 함께 사용하고, 실제 필요한 DP
노드 수 및 동시에 살아 있는 두 union 단계를 포함한다. 128개 이상의 노드에서
무조건 `2^n`을 계산하여 overflow하는 경로를 피한다.

구현·회귀: [Build probability](../../crates/clearra-core-executor/src/backend/wasm_cpu/build_probability.rs),
[실행 가지치기 회귀](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup/execution_pruning_tests.rs).

### 공급 변화: 같은 StoreCurrent 전이를 여러 간선에서 다시 계산하지 않음

빈 Hold에서 `StoreCurrent`를 선택한 뒤 같은 미노의 여러 언어 간선을 방문할 때,
공통 공급 전이를 간선 반복문 밖에서 한 번 계산한다. 간선 순서, 원래 입력 ordinal,
공급 종료의 Hold 방출, witness 및 memo 재활용의 의미를 유지한다. 같은 종류의
미노라도 CountAll에서 서로 다른 공급 동작의 multiplicity를 합치지 않는다.

P7은 각 종류를 한 번 공급하므로 이 변경의 충분한 검증 fixture가 아니다. 별도의
P3 210개 원래 입력, 고정/중복 미노 언어, Hold off/on, union과 workspace recycle을
비교했다. 공급 회귀 5개가 통과했고, GUI/desktop의 공급 길이 1→2 변경에서 실제
universe 7→42 및 covered 1→12 변화도 별도로 확인했다.

구현: [standard_bag_coverage.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage.rs).

### Setup: 선택된 prefix와 맞지 않는 후보를 저장 전에 제외

선택 detail의 정확한 board, 원래 삭제 행 mask, depth를 소비한다. 목표에 없는
원래 행이 이미 삭제됐거나, 목표 depth의 필드/삭제 집합이 다르면 이동 검사를
생략한다. 단순 board-subset 검사는 현재와 목표의 삭제 mask가 같은 좌표계일 때만
적용한다. 앞으로 줄 삭제가 남았을 때는 셀이 사라지거나 내려갈 수 있으므로 이
검사를 적용하지 않는다. 선택 depth 이후 continuation에는 prefix 제한을 넘기지
않는다.

모든 Setup 경로에서 placement identity 저장은 첫 허용 pose가 확인될 때까지
미룬다. 실패한 row가 검색 종료까지 살아 있는 placement-set에 들어가지 않는다.
이는 앞선 소스 초안에서 구현한 개선을 이번에 실제 회귀 4개로 검증한 부분이다.

구현: [setup_partial_build.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/setup_partial_build.rs).

## 3. GUI·진행 표시·협력적 GC

| 항목 | 변경과 유지한 계약 |
|---|---|
| 베타 버튼 | PC/Build의 `미노 의존성 사전 분석(베타)` UI와 문구 삭제. 과거 저장 초안의 `true`도 실행 시 무시하고 optional prepass를 끔. 후보별 정확 BuildUp·공급·커버리지 처리는 계속 사용 |
| Recovery `-` | 제거 가능 조건을 중간 필드 수 `>1`로 수정. 5→4→3→2→1 반복 감소 및 삭제 시 남은 필드·공급의 출처 보존 검증 |
| 비활성 초기 B2B | 실제 PC 점수 제품에서만 raw 값을 검증. 일반 탐색은 초안에 남은 NaN·음수 등에 막히지 않고 실행 값 0으로 정규화. Build의 기존 점수 제품 한정 처리는 유지 |
| GPU·Auto 정리 | 사용하지 않는 GUI GPU prewarm/validation 경로 삭제. 공유 API와 비GUI backend 기능은 보존. CPU Auto, 전체 논리 CPU, Recovery의 선행 설치 수 auto와 Setup auto는 실제 사용 기능이므로 유지 |
| 진행 이벤트 | UI가 느리거나 멈췄을 때 모든 Progress를 축적하던 큐를 latest slot으로 변경. Started·Diagnostic·terminal 경계의 순서와 취소/완료 전달은 보존 |
| Node GC | 루트 main isolate만 정확한 PID·request ID에 ACK. 같은 PID를 쓰는 worker isolate는 ACK 불가. 파일 감시 생성/동작 실패 때 1초 polling으로 요청을 처리하고 watcher/timer는 `unref()` |
| 관리자 실행 | 명시적인 `--no-timeout`을 로컬 build-test/verification/benchmark-search에 한정. 메모리·프로세스 트리·출력 제한, 압박 시 회수 요청과 안전 종료, 자동 자원 변경 재시도 금지는 유지 |

진행 이벤트는 100,000회 stalled-consumer 및 여러 producer 회귀를 포함하여 검증했다.
GC 5개 회귀는 정확 ACK와 worker ACK 차단, watcher 실패 후 polling, GC가 없는
루트의 ACK 금지를 확인했다. Node main event loop가 동기 작업으로 막혀 있으면
즉시 GC할 수 없으며, main isolate ACK는 Rust owner, WASM linear memory나 다른
worker heap이 회수됐다는 뜻이 아니다. Rust 자료는 소유권 해제로 수명을 관리한다.

관련 구현: [Recovery](../../packages/clearra-ui/src/lib/workspace/recoveryStages.ts),
[PC 입력 모델](../../packages/clearra-ui/src/lib/workspace/solverWorkspaceModel.ts),
[진행 이벤트](../../crates/clearra-gui-host/src/job/gui_job_runner.rs),
[GC responder](../../scripts/runtime/clearra-node-gc.cjs),
[manager](../../tools/clearra-manage/src/runtime.rs).

## 4. 재생 결과의 메모리 수명과 계수 오류 수정

완전한 scoring graph를 가진 paged PC Path에서 B2B 후처리가 예정되어 있고 Save가
아니면, 사용하지 않는 전체 eager replay를 먼저 만들지 않는다. 취소·예산 검사,
B2B 필터와 audit는 계속 실행한다. 완료 응답에서 producer scoring batch와 eager
replay를 정리하고, 페이지 소스가 자신에게 필요한 그래프와 manifest를 소유한다.

이 과정에서 기존 Path 검증기가 서로 다른 두 계수 계약을 혼동하는 오류를 찾았다.
B2B 필터 후 원시 build multiplicity는 `0/false`, 즉 정확한 수를 모르는 상태로
남을 수 있다. 완성되어 실제 결과에 결합된 replay manifest는 별도로 정확한
canonical witness 수를 제공한다. 완료된 manifest가 있으면 그 수를 사용하고,
eager 경로에는 기존 exact raw count 요구를 유지하도록 수정했다. 공통 완료·
CountAll·Trace·원래 universe 및 source binding 검사는 유지한다.

공개 App API 10개 사례에서 늦은 B2B 필터+전체 전개 기준과 모든 페이지의 witness,
각 step, 원래 입력 ordinal 및 coverage가 일치했다. `IIOOO`는 6,144개 witness를
끝까지 대조했다. 응답을 해제한 뒤에도 별도로 보유한 page source로 모든 페이지를
읽을 수 있고, 마지막 강한 참조를 해제하면 관찰용 `Weak`가 승격되지 않았다.

| IIOOO / TSpins의 논리 자료 용량 | byte |
|---|---:|
| B2B off 참조 그래프의 필터 전 전체 eager owner | 46,288,896 |
| 실제 B2B on 완료 응답의 Core | 39,730 |
| 실제 B2B on 완료 page source | 220,065 |

첫 행은 이전 B2B on 제품이 실제로 보유했던 메모리의 측정값이 아니다. 따라서
위 표를 이전 대비 RSS 절감률로 계산하면 안 된다. 그래프 복사 중의 중첩 수명과
페이지 방문용 임시 자료도 별도로 남는다. 상세 정의, 10개 사례와 실패 처리는
[App 재생 수명 검증](app-replay-lifetimes-2026-11-24.md)에 있다.

GUI Path는 현재 B2B 토글이 비활성화된다. 이 개선은 정확한 B2B Path를 요청할 수
있는 공통 App API의 개선이며, GUI에서 해당 토글을 새로 노출한 작업이 아니다.
일반 GUI Path도 별도 WASM facade 회귀를 통과했다.

## 5. P7 옵션·공급 대조 결과

### Core의 semantic-pruning off/on

고정 보드 `0x3c0f03c0f`, 4줄, 6개 배치, 원래 P7 5,040개 입력을 사용했다.
규칙 5종 × Hold 2종 × CountUnique/CountAll의 20개와 추가 spin 프로필 5개,
총 **25개 사례**가 통과했다. 양쪽 모두 요청의 B2B 제약을 유지하고 내부 조기
가지치기의 off/on을 비교한다. 이 25개를 사용자 B2B 토글 on/off로 설명하면 안 된다.

완전한 key·global/per-identity coverage·원래 pattern/cursor/Hold·CountAll·확률·
완료 상태·제약 만족 경로 및 witness를 비교했다. SRS+ / Hold / CountAll은 원래
커버리지 5,040 및 variants 4,011,520을 보존했다. NoKick / Hold / CountAll은
covered 5,024 및 variants 2,079,232로 규칙별 차이도 보존했다.

### 실제 App/WASM의 사용자 토글

같은 P7 보드, SRS+, 빈 Hold 활성, CountUnique에 대해 사용자 B2B와 개별 확률을
2×2로 비교했다. 작은 결정적 기능 검증이므로 worker는 1이다.

| B2B 보존 | 개별 해 확률 | 최종 identity | 성공 원래 입력 / 전체 |
|---|---|---:|---:|
| 꺼짐 | Skip | 246 | 5,040 / 5,040 |
| 꺼짐 | Include | 246 | 5,040 / 5,040 |
| 켜짐 | Skip | 4 | 724 / 5,040 |
| 켜짐 | Include | 4 | 724 / 5,040 |

확률 표시를 바꿔도 전체 key·global bitmap이 같았다. Include의 행 커버리지 합집합과
확률 분모를 원래 5,040에 대해 확인했고, B2B on의 후보와 원래 ordinal 행은 off의
부분집합이었다. P7의 한계를 보완하는 공급 변경 회귀는 앞의 별도 P3·고정 큐·EOF·
desktop 공급 길이 검증이다.

원자료: [P7 Core 로그][p7-core], [WASM 실행 stdout][wasm-out],
[WASM 실행 stderr][wasm-log], [공급 회귀][supply-log].

## 6. P7P4 성능 실험

### 비교 조건과 결과 동일성

빈 4줄 필드, P7P4, 10개 배치/11개 공급, Jstris180, 빈 Hold 활성, CountUnique,
full-future-oracle, global-only 커버리지 출력을 사용했다. Auto/전체 논리 CPU 정책으로
할당·실제 활성 worker가 모두 9였으며 자원 축소 재시도는 없었다.

Release example은 `parallel,local-search-ab,wasm-stage-profiling` 구성으로 한 번
고정 빌드했다. 비계측 네 번은 stage/RSS 진단을 끄고, D0/D1만 켰다. 다른 실험 축은
모두 0, bag memo는 reference/adaptive/compact, absorb는 0으로 고정했다.

모든 정상 완료 실행은 다음 값을 공유했다.

| 항목 | 값 |
|---|---|
| 소스 | `49fd03da6255c024f71c92d21b305fe34ec99b94` |
| 바이너리 SHA-256 | `afffa1df5c3b2ca67f87c33d5f7e7146b1704c9ea8097b7ce163043b85713bea` |
| 정규화 identity 수 | 456,459 |
| 성공 입력 / 원래 universe | 4,233,600 / 4,233,600 |
| 정규화 tiling 해 집합 해시 | `cts1:2fcbc950acdffa31` |
| 전체 정렬 identity SHA-256 | `08c477a2f71455a72778f076f9942789a9997ab3b3008fc2a3d043f02458f062` |
| 전체 원래 입력 bitmap SHA-256 | `0d7794467640c727b89ccd5f170fc4430228e0aba6fa9d7f8657b0e653ef3912` |

Rust 분석기가 source marker, 실제 바이너리, 비실험 설정, worker 수, 완전성, 해·입력
digest와 정상 종료 receipt를 확인했다. 이 benchmark는 해별 coverage relation을
모두 출력하지 않으므로 global bitmap의 일치만으로 해별 relation 동치까지 주장하지
않는다. 그 경계는 작은 회귀와 피벗 선택 불변식으로 따로 검증한다.

### 비계측 검색 wall time

| 실행 | 최소 지지 수 조기 종료 | 검색 시간 | 관리자 관측 peak MiB |
|---|---|---:|---:|
| A1 | 꺼짐 | 149.336초 | 4,356.19 |
| B1 | 켜짐 | 156.995초 | 4,448.16 |
| B2 | 켜짐 | 148.765초 | 4,430.76 |
| A2R | 꺼짐 | 146.656초 | 4,396.17 |
| 기준 평균 | 꺼짐 | **147.996초** | — |
| 실험 평균 | 켜짐 | **152.880초** | — |

A2는 실행 도중 tool wait가 중단된 뒤 결과와 정상 종료 receipt를 확보할 수 없었다.
사용자가 허용한 고아 작업 취소 절차로 정확한 lock identity를 기록·퇴역하고 같은
설정의 A2R을 실행했다. **중단 없는 연속 ABBA 실험이 아니다.** A2의 빈 결과는
평균에서 제외했고 A2R을 자동 재시도나 원래 A2의 성공으로 기록하지 않았다.

원래 owner와 A2 모두 기존 controller를 다시 조회할 수 없고 정상 종료 receipt가
없어 실제 프로세스 트리 종료를 직접 확인하지 못한 상태를
`process_tree_stopped: null`로 보존했다. 재사용될 수 있는 숫자 PID에 신호를 보내지
않았다. 이는 정상 실행 receipt와 다른 사용자 승인 취소 기록이다.
정상 완료한 여섯 실행의 자체 receipt는 exit 0과 소유 tree 종료를 확인한다.

평균의 3.300% 차이는 기술 통계다. n=2/arm이고 범위가 겹치므로 유의한 일반 성능
열세나 개선을 확정하지 않는다. 분석기의 `timing_adoption_authority=true`는
비계측 관측의 입력/완료 검증을 통과했다는 뜻이며, 통계적 유의성이나 채택 권고가
아니다. 과거 고정 스냅샷의 raw report가 cutoff=1을 `product`라고 부르더라도
**최종 기본값은 cutoff=0**이다.

분석·원자료: [비교표][analysis-md], [전체 분석 JSON][analysis-json],
[실행 manifest][manifest], [실행 wrapper][wrapper], [Rust 분석기][analyzer].

## 7. 구간별 시간: 주요 비용은 검증 내부에 있음

D0/D1은 진단 전용으로, 아래 값을 위의 wall time 평균에 섞지 않는다.
`work_and_wait`는 caller가 직접 맡은 worker 계산과 나머지 worker 대기를 함께
포함한다. 이를 유휴 대기 시간으로 해석하지 않는다.

| caller의 구간 elapsed span | D0 / cutoff=0 | D1 / cutoff=1 |
|---|---:|---:|
| parallel plan | 0.791초 | 0.538초 |
| worker 초기화 | 0.000280초 | 0.000177초 |
| caller 작업 및 완료 대기 | 140.554초 | 138.246초 |
| worker 결과 merge | 0.293초 | 0.108초 |
| 진단 검색 전체 | 142.991초 | 140.034초 |

검증 내부의 주요 구간은 아래와 같다. caller와 pool worker의 **샘플 확대 누적
경과시간**이므로 단일 검색 wall time이나 CPU 사용 시간은 아니다. 부모·자식
span이 중첩되므로 더하지 않는다. 비교 진단의 시간 차이를 독립적인 speedup으로
채택하지 않는다.

| D0의 구간 | 확대 누적 추정 | 실제 측정 샘플 시간 합 |
|---|---:|---:|
| Build order 도달성 | 626.379초 | 8.785초 |
| coverage language product | 345.956초 | 5.023초 |
| 위 product 중 standard bag 계산 | 272.638초 | 4.153초 |
| 위 product 중 language 구성 | 71.192초 | 0.852초 |
| geometry advance | 20.563초 | 0.213초 |
| candidate projection | 10.452초 | 0.093초 |

`strict.standard_bag_product` 같은 내부 span 이름은 일반 제품 경로에서도 쓰인다.
이 이름만으로 strict 최적화 실험이 활성화되었다고 판단하지 않는다. 진단은 실제
측정 시간/횟수와 확대 추정치를 별도 필드로 남긴다.

이 결과에서는 기하 피벗 선택만 줄이는 변경이 큰 검증 비용에 비해 제한적인
영향을 가진다는 해석이 가능하다. 단, scheduling과 캐시가 달라질 수 있으므로
조기 종료의 관측 차이를 특정 원인 하나로 단정하지 않는다.

구현: [stage profiler](../../crates/clearra-core-executor/src/performance/search_stage_profiler.rs),
[benchmark 진단](../../crates/clearra-core-executor/examples/placement_graph_bench/stage_diagnostic.rs).

## 8. 구간별 메모리: 자료의 소유자와 수명을 구분

아래 GB/MB는 10진 byte 단위다. RSS는 25 ms 표본이며, 각 phase의 첫/마지막 값은
관측값이지 정확한 경계 시각의 값이 아니다.

| 구간 | D0 관측 peak RSS | D1 관측 peak RSS | 주요 보유 자료 |
|---|---:|---:|---|
| 사전 dispatch | 3.342 MB | 3.375 MB | 작은 요청·프로세스 초기 상태 |
| 기하 준비 | 15.766 MB | 15.987 MB | catalog, residual memo, solution family |
| 병렬 검증 | **4.679 GB** | **4.645 GB** | worker별 bag, 순서 언어, 도달성, 반환할 identity |
| merge | 1.468 GB | 1.558 GB | worker 반환값, 병합 aggregate, 대표 경로 재검증 |
| 결과 마무리 | 1.175 GB | 1.019 GB | 정규화 key·identity·결과 벡터 및 후속 자료 |

D0 마지막 결과 마무리 관측은 **1.091 GB**, D1은 **0.915 GB**다. 위의 마지막
행은 구간 최고치이므로 마지막 관측값과 구분한다. D0 kernel HWM은 4.688 GB,
D1은 4.658 GB로, 25 ms sampler peak와 일치할 필요가 없다. 관리자 process-tree
peak도 측정 범위·시점이 달라 별도 값으로 보존한다.

### 검증 peak에 연결된 논리 자료 용량

| worker별 마지막 공개값의 합 | D0 | D1 |
|---|---:|---:|
| 표준 bag 전체 | **3,448.430 MB** | **3,455.933 MB** |
| bag 안의 union memo payload | 1,585.447 MB | 1,585.447 MB |
| bag 안의 product memo payload | 653.857 MB | 661.360 MB |
| 미노 순서 언어 | 530.702 MB | 528.965 MB |
| 도달성 자료 | 40.208 MB | 40.715 MB |
| 결과 identity | 84.673 MB | 89.653 MB |
| graph projection | 0.849 MB | 0.849 MB |

union/product memo는 bag 전체의 **부분집합**이다. 이를 bag에 다시 더하지 않는다.
bag의 나머지에는 decision nodes, summaries, levels, masks, buckets, directory,
global words 등이 있다. 작은 shared table은 요청 owner에서 한 번만 센다.

D0의 중복 제외 worker 논리 합은 4,104,990,728 B다. worker 공개값은 peak 시점보다
16–626 ms 전이며 D1의 가장 오래된 공개값은 1,075 ms 전이다. 즉 완전히 동시점인
할당 계측이 아니다. Coordinator의 dispatch 공개값도 오래 유지되므로 peak 순간의
전체 coordinator 자료를 새로 센 값으로 읽으면 안 된다.

### worker 종료와 결과 이동

worker는 종료 직전 용량 수치를 남기고 `BuildUpWorkspace`와
`CoverageProductEvaluator`를 해제한 뒤 identity·branch 자료를 coordinator로
넘긴다. D0의 종료 직전 scratch 합 4,029.485 MB는 서로 다른 worker 종료 시점의
기록이다. merge에 그 scratch가 계속 남아 있다는 의미가 아니다.

merge는 worker 결과를 소비하고 첫 벡터의 소유권을 넘겨받으며 이후 결과를 append/
union한다. 원본과 대상 용량이 잠시 함께 존재할 수 있다. 관측기는 merge 진입 시
worker 공개 슬롯을 비우므로 이때 worker 계측값 0을 전체 메모리 0으로 해석하지
않는다.

Coordinator handoff result slots는 79,692,288 B, terminal assembly는
80,866,304 B다. 이는 선택한 벡터의 예약 슬롯 용량이며 중첩 문자열, coverage
backing, typed proof, 호스트 인코딩 전체를 포함하지 않는다. 동일 벡터가 두
checkpoint를 이동할 수 있어 두 값을 더할 수 없다. **마무리 RSS와 이 슬롯 용량의
차이를 누수량이나 allocator 잔류량으로 단정할 근거는 없다.**

소유권/정의: [메모리 관측기](../../crates/clearra-core-executor/src/backend/wasm_cpu/parallel_memory_peak.rs),
[worker](../../crates/clearra-core-executor/src/backend/wasm_cpu/parallel_worker.rs),
[merge](../../crates/clearra-core-executor/src/backend/wasm_cpu/parallel_search.rs),
[result](../../crates/clearra-core-executor/src/backend/wasm_cpu/result.rs).

## 9. 추가 구조 개선의 평가

피벗 조기 종료는 이미 가장 좋은 셀의 지원 후보 수를 얻은 뒤, 뒤쪽 셀의 수가 그
값 이상이 되는 순간 나머지 incidence 검사를 생략하는 방식이다. 첫 동률 선택과
늦게 나오는 0-support를 보존하며, 새로운 캐시를 추가하지 않는다. 128개 미노 mask와
폭 4/7/10의 초기 필드·빈틈·공급 제한/소진/복원 회귀를 통과했다. 이번 P7P4 결과로
기본 채택을 철회했으나 실험 경로는 남겨 향후 다른 구조에서 평가할 수 있다.

큰 메모리를 줄일 다음 후보는 **전체 원래 입력 bitmap이 정확히 포화된 뒤의 후보
membership 계산을 분리**하는 것이다. 현재 `MembershipOnly`는 concrete 단일 큐에
한정되므로 symbolic P7P4는 계속 full coverage root를 계산한다. 캐시를 즉시
버리면 이후 후보가 쓰는 자료를 없애거나 반복 계산을 일으킨다.

검토한 후속 설계는 이미 물리 검증된 candidate-local `PieceOrderLanguage`와 기존
공급/Hold transition을 소비하여 최소 원래 ordinal 또는 불가능만 구하는 visitor다.
family DAG 직접 풀이를 도입하는 설계는 아니다. 원래 입력 순위, EOF Hold, 모든
solution identity와 대표 witness 동률을 보존해야 하며, CountAll·개별 행 확률·최소
포트폴리오·실패 큐·score/spin/Finesse·Visible 7 등의 소비자는 초기 적용에서
분리해야 한다. 수천만 상태에 새 HashMap을 겹쳐 얹으면 현 compact memo보다 더
커질 수 있어 안전한 자료형 전환·보유 한도부터 검증해야 한다.

이번 실행은 정확 포화 시각을 계측하지 않았으므로 절감 가능한 시간·RSS를 예측하지
않는다. 이 설계는 **미구현 연구 후보**다. 이미 적용된 identity 이동과 workspace
해제를 다시 새 최적화로 계산하지 않는다. 수치 score가 낮다는 이유만으로 가지를
버리는 것도 미래 bonus를 포함한 정확한 상한 없이 추가하지 않는다.

## 10. 실제 검증과 남은 범위

| 구분 | 실제 결과 | 근거 |
|---|---|---|
| GUI 최종 회귀 | 54 통과 | `gui-final-tests.log` |
| Build/Recovery 옵션 | 26 통과 | `build-recovery-option-tests.log` |
| UI TypeScript 계약 | 종료 0 | `gui-final-typecheck.log` |
| Node GC | 5 통과 | `pinned-node-gc-tests.log` |
| 벤치 드라이버 격리 | 28 통과 | `diagnostic-isolation-tests.log` |
| Scoring 정책 | 4 통과 | `scoring-final-tests.log` |
| Core 선택 회귀 | 35 통과, P7 ignored는 별도 실행 | `core-fixture-corrections.log` |
| 공급 전이 | 5 통과 | `core-supply-transition-final.log` |
| Core P7 | 1 테스트 안의 25 사례 통과 | `p7-option-matrix-final.log` |
| Desktop host | 10 통과 | `desktop-final-tests.log` |
| WASM facade | 앞선 10개 통과 + 보정/신규 4개 최종 통과 | 아래 설명 |
| App 공개 API | 10 사례의 모든 witness 및 수명 통과 | `app-public-validation-v3.json` |
| App private 신규 4개 | 타입 검사 통과, body 미실행 | `app-test-typecheck.log` |
| example 진단 | 5 통과 | `stage-diagnostic-example-tests.log` |
| 최종 기본값·경고 보정 | Core 4개 + example 5개 통과, 두 빌드 경고 없음 | `core-cutoff-default-clean-final.log`, `example-cutoff-default-clean-final.log` |
| P7P4 | 비계측 4회 + 진단 2회 정상 완료/동일성 통과 | `p7p4-manifest.json` |

WASM facade의 첫 12개 묶음은 10개 성공, 두 오래된 테스트 계약 실패였다. 이후 실제
호스트 CPU 상한과 최소 포트폴리오/원래 전체 후보의 계약을 구분하도록 검사를
보정했다. 신규 P7 2×2와 일반 Path를 포함한 묶음에서 3개가 통과했고, 최소 제품의
참조 실행에 빠졌던 `--solution-probabilities`를 추가한 마지막 실행에서 나머지 1개가
통과했다. 실패 로그를 삭제하지 않았다. 이는 native에서 WASM facade 라이브러리를
실행한 검증이며 브라우저 WASM 인스턴스의 E2E 조작을 의미하지 않는다.

App 전체 libtest 바이너리 생성은 같은 구성에서 두 번 rustc SIGKILL로 종료됐다.
manager가 관측한 peak는 각각 7,839,506,432 B와 7,608,053,760 B였으나 강제 종료
사유·memory pressure event가 없고 cgroup OOM 증분은 unknown이다. **확정 OOM으로
기록하지 않는다.** 자원을 바꿔 monolithic 실행을 재시도하지 않았다. 실제 production
rlib에 연결한 공개 API 하네스 10개와 private test 타입 검사를 대안으로 수행했으며,
이를 App 전체 private suite 성공으로 대신 표시하지 않는다.

처음 native libtest 기본 스택에서 난 stack overflow도 보존했다. 이후 native 제품
호출 경계의 기존 16 MiB 스택으로 WASM facade를 검증했다. 이는 OOM의 worker 축소
재시도나 브라우저 메모리 검증이 아니다. package/lock 입력 8개의 SHA는 유지됐다.
전체 frontend build, 실제 브라우저/Tauri 클릭, Discord 전달, 모든 옵션 Cartesian
조합은 이번 검증 범위에 포함하지 않았다.

최종 cutoff 기본값 복원 뒤 Core 4개, example 5개를 다시 실행했다. local-search-ab
빌드의 미사용 함수 경고는 기준 arm이 실제 제품 함수를 직접 호출하고 독립 full-scan
reference helper를 test에 한정하도록 정리했다. 보정 후 두 실행은 경고 없이 통과했고,
정상 종료 receipt는 각각 `1795496746206404686-5`, `1795496807941191563-5`다.
반복된 회귀를 별개의 신규 테스트로 더하지 않는다. 지정 파일 rustfmt,
`git diff --check`와 package/lock 입력 SHA 일치도 확인했다. 측정 바이너리는
고정 스냅샷의 파일을 보존한다.

## 11. 재현 자료와 수명 종료 기록

원자료는 `_local/artifacts/gui-option-optimization-20261124/` 아래에 있다. 실행
wrapper는 한 번의 작업만 실행하고, manager의 receipt·source marker·바이너리
SHA·launch 설정을 보존한다. Rust 분석기는 수집된 JSON을 검증할 뿐 solver를
다시 실행하지 않는다. 대형 App 디버그 실행 파일은 텍스트 증거 묶음에 넣지 않는다.

정상 receipt, 실패한 compile/run, 수동 orphan 취소는 서로 다른 상태로 보존한다.
원래 고아 실행 `1795475629633014911-5`와 중단된 A2
`1795495225767792107-27`의 취소 기록은 정상 종료 증거로 취급하지 않는다.
이 구분은 사용자의 강제 취소 허용을 적용하면서 관측하지 않은 tree 종료를
만들어 기록하지 않기 위한 것이다.

[p7-core]: ../../_local/artifacts/gui-option-optimization-20261124/p7-option-matrix-final.log
[wasm-out]: ../../_local/artifacts/gui-option-optimization-20261124/wasm-final-targeted-v2.out
[wasm-log]: ../../_local/artifacts/gui-option-optimization-20261124/wasm-final-targeted-v2.log
[supply-log]: ../../_local/artifacts/gui-option-optimization-20261124/core-supply-transition-final.log
[analysis-md]: ../../_local/artifacts/gui-option-optimization-20261124/p7p4-analysis.md
[analysis-json]: ../../_local/artifacts/gui-option-optimization-20261124/p7p4-analysis.json
[manifest]: ../../_local/artifacts/gui-option-optimization-20261124/p7p4-manifest.json
[wrapper]: ../../_local/artifacts/gui-option-optimization-20261124/run_product_bench.mjs
[analyzer]: ../../_local/artifacts/gui-option-optimization-20261124/bench_analysis.rs

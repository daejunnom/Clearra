# v0.9.0 Rust 제품 경로와 호스트 동등성 검증 계약

2026-11-20 KST · F01/F03/H01 소스 감사 · 기준 `16d5acfadf55c8e50824f36037093a85286955d7`.
아래의 **연결됨**은 현재 소스에 입력→실행→결과 경로와 해당 fixture가
있다는 뜻이다. 이 감사에서는 빌드·테스트·배포를 실행하지 않았다. 예전
성공 기록은 이 SHA의 CI·성능·제품 배포 증거가 아니다.

## 제품 호스트의 실제 경계

| 호스트 | 현재 경로와 책임 | 판정 시 남길 증거 |
| --- | --- | --- |
| 배포 CLI | [`product_app_context`](../../crates/clearra-cli/src/cli_routing.rs#L382)는 `wasm-cpu-runtime`이면 `AppCoreExecutorService::wasm_cpu()`, 아니면 기본 `AppContext`를 선택한다. [Linux 패키징](../../scripts/tools/package-release-cli.sh)은 `wasm-cpu-runtime,webgpu-search`로 빌드한다. [릴리스 워크플로](../../.github/workflows/release-cli.yml)의 C 기반 ProductE2E 입력과 독립 CLI 패키지는 같은 바이너리가 아니다. CPU 전용 검증은 요청에서 CPU를 선택한다. | 실제 실행 파일 digest, 기능 설정, source/engine identity, backend 선택, JSON 결과. 원래 exe로 직접 실행한 결과와 호스트가 재포장한 결과를 구별한다. |
| Discord | [Direct executor](../../apps/clearra-discord-bot/src/clearra/direct-executor.mjs)와 [Cloud job service](../../apps/clearra-discord-bot/src/job-service/main.mjs)는 공통 [`ClearraCommandRunner.execute`](../../apps/clearra-discord-bot/src/job-service/runner.mjs#L65)를 쓴다. Runner가 워커/기한, JSON, solution data를 적용하고 설정된 CLI를 spawn한다. [승인된 job image](../../apps/clearra-discord-bot/Dockerfile.accepted-job-service)는 이미 빌드된 Linux CLI를 복사한다. Discord formatter의 canonical-only 결과와 CLI 전체 포트폴리오/페이지 결과는 의도적으로 다르다. | 실제 image에 들어간 승인 CLI digest, adapter 전후 argv, 결과 kind와 정규화한 canonical projection. 가벼운 직접 실행 fixture를 Cloud 배포·등록·동기화 확인으로 대신하지 않는다. |
| Web / Pages | [`ClearraProductJobRunner.run`](../../apps/clearra-web/src/workers/ClearraProductJobRunner.ts#L33)은 제품 자원 lease 아래에서 분산 준비를 하고, 준비 결과 그대로 완료하거나 `DistributedWasmJobRunner` 또는 `WasmJobRunner`를 선택한다. [WASM runtime 기본 App](../../crates/clearra-wasm/src/wasm_command_runtime.rs#L4408)은 Rust CPU다. [WASM ABI](../../crates/clearra-wasm-abi/src/lib.rs)의 준비·worker transfer·검증·완료는 호스트 스케줄링 경계이며 native Rust의 스레드가 아니다. | 게시 대상 WASM digest·source/engine identity, 전송된 실제 작업/검증 이벤트, 요청 및 실제 워커, terminal payload·모든 페이지·복사 결과. Node Worker smoke, 실제 브라우저, Pages readback은 각각 구분한다. |
| Desktop | [Tauri 진입점](../../apps/clearra-desktop/src-tauri/src/main.rs)은 `run_request`, `start_job`, `cancel_job`, 이벤트와 페이지 IPC를 등록한다. [Desktop bridge](../../crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs#L35)는 CPU runtime feature에서 Rust App을 사용할 뿐 CLI 프로세스를 spawn하지 않는다. [Tauri Cargo](../../apps/clearra-desktop/src-tauri/Cargo.toml)는 이를 명시해 빌드한다. | 실제 bridge의 job 완료와 page/copy의 유효 scope, 패키지 digest, 실제 창/IPC readback. bridge를 직접 호출하는 테스트는 Tauri 창을 검증하지 않는다. |

계산 계약의 소유자는 호스트가 아니다. 규칙·킥은 `clearra-rules`, 표준
형상은 `clearra-piece-registry`, 요청·실행 정책은 `clearra-problem` 및
Core domain, ILC·검증은 `clearra-core-executor`, 공급과 성공 큐 집합은
기존 supply·coverage 구현, 제품 기능·완결성은 `clearra-app`이 소유한다.
호스트는 요청 변환·전달·스케줄링·표시를 소유한다. C FFI를 통한 ID
인코딩 의존성, 실제 C 계산 호출, 외부 ABI 소비를 각각 확인한다. 별도
legacy Cover는 [App 서비스](../../crates/clearra-app/src/app_services.rs)의
C 경로와 Rust의 **증명된 빈 물리 결과** 경로만 비교 가능하다. 기존
Build Probability의 성공을 일반 양성 legacy Cover의 증거로 쓰지 않는다.

## 1–24L: 입력 수용·실행·결과·호스트 동등성 구분

[`StandardPcBoardStorageKind`](../../crates/clearra-core-domain/src/board/standard_pc_board.rs)는
1–6L을 Board64, 7–12L을 Board128, 13–24L을 Board256으로 구분한다.
외부 확장 문서는 canonical 네 워드로 보존한다. 점유 높이와 목표 높이가
다를 수 있으며, [컴파일 경계](../../crates/clearra-problem/src/extended_pc_search_contract.rs#L73)는
원래 행/초기 clear/목표 프레임을 유지한다. 빈 Opening은
짝수 2–24L, 명시된 초기 필드 Scenario는 면적·공급 조건이 맞는 홀수도
가능하다. 24행 인코딩이나 빈 Opening의 예산 거절은 전체 열거의
완주 증거가 아니다.

| 가족 | 이 SHA에서 연결된 실행·출력 | 확대해서는 안 되는 범위 |
| --- | --- | --- |
| Compact PC (1–6L) | 기존 compact ILC, BuildUp, 공급, reducer. CTK1을 유지한다. | 소형 CEGIS/BFS 회귀를 확장 Board128/256 성능 또는 증거로 쓰지 않는다. |
| 7–24L Tiling | 별도 `ExtendedPcTilingSession`, geometry/tiling family 결과 및 확장 키. native 병렬은 `parallel` feature, Web은 별도 typed 작업 경계. | Tiling은 `buildup_executed=false`이고 성공 큐 확률 증거가 없다. 부모/worker의 원시 패킷은 제품 결과 권위를 발급하지 못한다. |
| 7–24L PC family | [`ExtendedPcSearchSession`](../../crates/clearra-core-executor/src/backend/wasm_cpu/extended_pc_search.rs)은 연결된 Opening·Scenario에서 ILC → BuildUp → 공급을 실행한다. ordinary All/Unique, chance/failed-queue, minimals와 pinned, score/score-minimals/score-finder, path의 소형 제품 fixture가 존재한다. 결과의 해 키는 CTK2, full-height trace 키는 TRK2 형식을 쓴다. | 각 제품의 objective/count/출력·증거 계약이 다르다. `VisibleSeven`은 연결된 ordinary family의 별도 검증이며 typed score/chance/minimum/replay까지 자동 적용되지 않는다. 풀 큐 oracle로 조용히 정규화하지 않는다. |
| 7–24L 워커·실패 | native parallel의 별도 확장 family 소유자와 Web의 별도 four-word transfer 권위가 있다. 요청 모드가 연결되지 않으면 검증/typed 오류로 끝난다. | [compact packed-worker의 `distributed_execution_is_worthwhile`](../../crates/clearra-core-executor/src/backend/wasm_cpu_search_backend.rs#L449)가 >6에서 false인 사실은 확장 Web 전송 전체의 미연결을 뜻하지 않는다. 반대로 native 병렬 지원 플래그가 모든 typed 요청·자원 제한의 병렬화를 보장하지도 않는다. |

Extended PC에서 명시 TB, precomputed Build Dependency DAG, 지원하지
않는 closed 옵션 조합, custom/Wide를 24L 표준 PC라는 이유로
받아주지 않는다. 기능의 단순 부재와 정확한 빈 해 집합, 취소,
예산/메모리 실패를 분리한다. 잘림·미완료에서 성공/확률/완결 key
집합을 출력하면 안 된다. 증거 요구가 없는 반례에 대해 무거운
replay나 모든 hold variant를 만들어 비교하지 않는다.

## 공유 fixture와 비교 행

**공통 소형 기준:** 폭 10, 높이/목표 1, 초기 `0x3f0`(아래 행의
열 4–9가 점유됨), 고정 큐 `I`, hold 없음, exact 1 placement,
SRS+를 쓴다. ordinary는 Unique/CountUnique, 전체 큐 oracle, CPU, 1워커,
TB/자산 off로 고정한다. CLI의 canonical 실행 요청 예:

```text
clearra --format json --include-solution-data pc --lines 1 --height 1 --board-mask 0x3f0 --pieces 1 --queue I --no-hold --rule srs-plus --objective unique --count unique --queue-knowledge oracle --backend cpu --workers 1 --no-tablebase --no-legal-board --no-conditioned-reachability
```

정답은 직접 계산할 수 있다. 유일한 가로 I가 `0x0f`를 채운 뒤 한 줄이
삭제되어 필드가 빈다. canonical key
`ctk1|initial=00000000000003f0|placements=I:000000000000000f`,
원래 큐 0의 가중 분자/분모 1/1이다. 같은 입력으로 제품별 모드를
검증할 때는 공통 직렬 argv의 단순 접두사 치환 대신 해당 closed
command builder를 사용한다. 예컨대 `tiling`에 rule/TB 옵션을
요구하거나 `score-finder`에 oracle/출력 제어를 강제로 주입하지 않는다.
Desktop/frontend에서 이미 생성하는 요청과 Discord가 실제 쓰는
옵션을 변경 없이 받아서 요청에 남는 의미를 비교한다.

**확장 고정 기준:** [공통 TSV](../../tests/fixtures/contracts/extended_pc_surface_input.v1.tsv)
의 `forced-7L/8L/12L/24L`은 실제 상위 행의 점유·클리어를 사용하고,
고정 큐 `II/II/III/IIIIII`, no hold, 2/2/3/6 placement만 갖는다.
SRS+를 공통 smoke로 두며 호스트 fixture의 다섯 프로필 행렬은 그대로
사용한다. 일반 성공과 별도로 `failed-queue`의 실패 큐(해당 fixture의
queue를 O로 교체), preserve-B2B의 7L exact empty, 24L 사용·24행 밖
bit의 입력 거절을 분리한다. 7L fixture는 3+4 clear가 필요하고
8/12/24L fixture는 Tetris-only이므로 B2B 확인의 좋은 짝이다.

| 검증 행 | 동등해야 하는 것 | Fixture/증거 소유자 |
| --- | --- | --- |
| Canonical ingress / identity | 모드·프로필·줄 수·원래 필드·고정 큐·hold·count·oracle·지원/거절 분류, full key의 네 워드·피스 배치·순서. raw JSON과 presentation의 문자열/숫자 차이는 기존 typed 어댑터에서만 정규화한다. | [공통 parser→App](../../crates/clearra-cli-command/tests/extended_pc_surfaces.rs), [실제 CLI](../../crates/clearra-cli/tests/extended_pc_tiling.rs), [격리된 Desktop bridge](../../crates/clearra-cli/tests/desktop_full_height_products.rs). |
| Ordinary / coverage | 전체 순서화한 normalized keys, 성공 큐의 정확한 집합·가중 분자/분모·완결성과 실패 큐 complement. Tiling은 기하 집합만 비교하며 `probability_calculated=false`를 요구한다. Visibility는 oracle과 결과가 같더라도 별도 정책 실행 증거를 요구한다. | [App/BFS 관측 fixture](../../crates/clearra-cli-command/tests/extended_pc_observation.rs), [실제 WASM Worker smoke](../../apps/clearra-web/test/realVerifierPool.smoke.mjs#L460), small과 P7P4의 per-solution 동등성은 새 별도 캡처/비교기가 소유. |
| Minimals, pinned, score | 최적 cardinality·정규화한 member 순서·집합/후보-map identity·완결성, 요청한 페이지와 전체 export; pinned는 실제 drawing과 source set에 결박. 점수는 full key/score/attack과 winner tie 선택. Discord는 canonical projection을 CLI의 동일 projection과 비교한다. | [Desktop actual family](../../crates/clearra-cli/tests/desktop_full_height_products.rs#L223), [Discord actual runner](../../apps/clearra-discord-bot/test/realCliFullHeightProducts.test.mjs#L146), [Web serial↔3-worker 및 lazy copy](../../apps/clearra-web/test/realVerifierPool.smoke.mjs#L516). |
| Path/replay와 자원 | canonical witness, 증거·전체 witness count, before/lock/clear의 모든 마스크, 최종 empty, lazy page/export. 500ms GIF 표시도 따로 확인. 예산 거절·취소 시 이전 결과를 완료로 재사용하지 않는다. | 같은 App/Discord/Web 제품 fixture, CLI의 explicit 1-MiB extended admission 거절, 실제 GUI 및 배포 검증은 별도 소유자. |

1워커 요청과 각각의 실제 병렬 확인은 별도 행이다. native 일반
1↔2워커와 Web 1↔3워커는 같은 canonical 결과를 비교하되, 워커 사용·
전송 여부·deadline·프로세스 성능은 별도 관측이다. 기하가 빈 작은 요청을
자동으로 1워커로 처리한 사실을 다중 워커 증거로 세지 않는다.
[`run-real-web-verifier-pool-smoke.mjs`](../../scripts/tools/run-real-web-verifier-pool-smoke.mjs)
및 [호스트별 기존 CI owner](../../.github/workflows/v081-selective-source-ci.yml)는
소형 실행 재사용의 출발점이다. 최종 승인에서는 전환한 exact SHA/빌드
산출물로 실제 네 표면을 확인하고 기존 v0.8.1 성능/자산 또는 v0.10.0 TB
트랙의 완료로 확대하지 않는다.

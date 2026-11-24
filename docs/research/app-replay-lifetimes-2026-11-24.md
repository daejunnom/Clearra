# App 재생 경로와 메모리 소유권 검증 — 2026-11-24

검토 기준 소스: `49fd03da6255c024f71c92d21b305fe34ec99b94`.

**공개 App API를 실제 실행한 10개 사례가 모두 통과했다.** B2B 보존을 켠 `AppContext::run`의 `pc.path` 결과를, B2B 보존을 끈 Core 그래프에 나중에 공개 B2B 필터를 적용하고 모든 재생을 전개한 기준 결과와 대조했다. 모든 페이지의 정렬된 witness와 각 단계, 원래 패턴 번호, coverage, 정확한 witness 수가 일치했다. 최종 Core 응답에는 eager replay와 scoring batch가 남지 않았고, 응답을 해제한 뒤에도 별도로 보유한 페이지 소스로 모든 페이지를 읽을 수 있었다. 그 마지막 강한 참조까지 해제한 뒤에는 `Weak::upgrade()`가 `None`이었다. [공개 검증 결과][results] · [실행 하네스][harness]

이 문서의 메모리 수치는 명시된 Rust 자료구조의 **논리적 보유 용량**이다. 실제 프로세스 메모리 최고치나 이전 제품 대비 메모리 절감량을 나타내지 않는다. 이 검증으로 Peak RSS 감소 또는 메모리 절감률을 주장하지 않는다.

## 1. 실행 근거와 범위

| 항목 | 확인한 내용 |
|---|---|
| 실행 대상 | 공개 `AppContext::run` → typed `PcPath` → 실제 `PcReplayPageSource` 및 모든 페이지 |
| 결과 | `passed=true`, `case_count=10`, 모든 사례의 `all_ordered_witnesses_and_steps_match=true` |
| 실행 영수증 | `1795494308792316924-5-runtime.json`: 종료 코드 0, 정상 종료, 소유 프로세스 트리 종료, 잔존 자식 0 |
| 링크 | `app-public-link-artifacts-v3.json`에 기록된 Cargo 산출 `.rlib` 사용. App 산출물은 `libclearra_app-320f3a9fcccb0176.rlib` |
| 빌드 성격 | `cfg(test)` 우회를 넣지 않은 실제 라이브러리에 독립 하네스를 연결한 native debug 실행 |
| 기능 구성 | App: `default`, `bitmap-render`, `parallel`, `online-pc4-tablebase`. Core executor: `default`, `parallel`, `local-search-ab`, `wasm-stage-profiling`. 정확한 전체 목록은 링크 기록에 보존 |
| 스택 | 최초 실행부터 native 제품 호출 경계에 맞춘 16 MiB 고정 스택 |
| 작업자 | 작은 결정적 fixture의 `workers=1`. 제품 성능 비교의 작업자 정책을 대신하지 않음 |
| 제품 보유 예산 | 성공 경로는 64 MiB. 실패 경로는 명시적으로 1 byte 예산을 사용 |

위 구성과 실행은 [링크 산출물 기록][link-artifacts], [공개 실행 로그][run-log], [runtime 영수증][run-receipt], [하네스][harness]로 확인했다. JSON도 `performance_authority=false`를 기록한다. 이 결과는 브라우저의 실제 WASM 인스턴스, GUI 조작 또는 Discord 전달 경로를 실행한 결과가 아니다.

## 2. 기준 결과의 독립성과 확률 분모

기준 쿼리는 피검증 App 쿼리와 보드·공급·홀드·규칙·CountAll 설정이 같고 B2B 보존 제약만 끈다. 공개 `ProblemCompiler`와 `WasmCpuSearchBackend`로 완전한 scoring graph를 만든 뒤, 공개 `BackToBackExecutionFilter`를 마지막에 적용한다. 이후 공개 `ExactScoringExecutionMaterializer::materialize_complete_replays_with_limits`로 모든 재생을 전개하여 기대 witness를 얻는다. 따라서 App의 조기 가지치기와 페이지 열거기를 그대로 기대값 생성에 재사용하지 않는다. [하네스][harness]

이 비교에서 기준 측과 피검증 측이 공유하는 것은 공개 Core와 B2B 판정의 제품 의미다. 서로 독립적인 외부 solver 두 개를 비교한 것은 아니다. 대신 조기 필터링과 늦은 필터링, 전체 전개와 지연 페이지 열거가 동일한 결과를 내는지 확인한다.

그래프만 보유한 Path Core 결과는 사용하지 않는 `postprocess_pattern_weights` 벡터를 비워 둘 수 있다. 이 벡터의 길이를 패턴 수로 간주했던 초기 하네스 가정을 수정했다. 현재 하네스는 **컴파일된 `MaterializedPatternUniverse`의 `weight_at()`과 `sequence_at()`**을 확률과 원래 ordinal의 근거로 사용한다. B2B on/off 쿼리의 원래 universe 수, 각 sequence, 각 weight가 같은지 확인하고, 각 scoring batch의 sequence도 원래 ordinal과 대조한다. [하네스][harness] · [초기 실패 로그][v1-log]

`[OI]O`/홀드 사례는 원래 두 패턴에 각각 `0.5`의 가중치가 있다. 성공한 원래 ordinal은 `[1]`이며 coverage는 `0.5`였다. 성공한 패턴만 남겨 분모를 다시 정규화하지 않았고, 최종 결과의 `original-materialized-queue` 분모 의미도 확인했다. [공개 검증 결과][results]

## 3. 10개 사례의 전체 재생 결과

모든 사례의 규칙은 SRS+다. `I`는 높이 1 / 보드 `0x3f0` / 1개 배치, `OO`, `II`, `[OI]O`는 높이 2 / 보드 `0xfc3f0` / 2개 배치, `IIOOO`는 높이 2 / 빈 보드 / 5개 배치다. 표의 원래 ordinal은 0부터 시작한다. [하네스][harness]

| Spin 정책 | 공급 | 홀드 | 원래 패턴 수 | 최종 geometry 수 | 필터 전 참조 witness | 최종 witness | 성공 ordinal / coverage |
|---|---|---|---:|---:|---:|---:|---|
| TSpins | I | 꺼짐 | 1 | 1 | 2 | 2 | `[0]` / 1.0 |
| TSpins | OO | 꺼짐 | 1 | 1 | 32 | 32 | `[0]` / 1.0 |
| TSpins | II | 꺼짐 | 1 | 0 | 4 | 0 | `[]` / 0.0 |
| TSpins | [OI]O | 켜짐 | 2 | 1 | 64 | 64 | `[1]` / 0.5 |
| TSpins | IIOOO | 꺼짐 | 1 | 4 | 6,144 | 6,144 | `[0]` / 1.0 |
| AllMiniPlus | I | 꺼짐 | 1 | 1 | 2 | 2 | `[0]` / 1.0 |
| AllMiniPlus | OO | 꺼짐 | 1 | 1 | 32 | 32 | `[0]` / 1.0 |
| AllMiniPlus | II | 꺼짐 | 1 | 0 | 4 | 0 | `[]` / 0.0 |
| AllMiniPlus | [OI]O | 켜짐 | 2 | 1 | 64 | 64 | `[1]` / 0.5 |
| AllMiniPlus | IIOOO | 꺼짐 | 1 | 4 | 6,144 | 6,144 | `[0]` / 1.0 |

표의 모든 값은 [공개 검증 결과][results]에서 옮겼다. `I`의 수를 근거 없이 1로 고정하지 않았고 실제 두 witness를 비교했다. `OO`는 같은 미노의 서로 다른 배치 순서를 보존했다. `II`는 PC가 아닌 일반 줄 지우기로 B2B가 끊기는 경로가 전부 제외되었다. `IIOOO`는 여러 geometry 및 여러 페이지를 끝까지 비교했다.

비교 항목은 canonical/producer candidate ID, 원래 pattern ID, trace identity와 정규화 키, 소비 수, terminal hold, 각 단계의 operation ID·active piece·cursor·hold decision·회전·좌표·배치 mask·배치 전후 보드·줄 삭제 후 보드·삭제 행과 개수다. 삭제 행은 기준 replay의 보드에서 다시 도출한다. 첫 canonical witness도 첫 paged witness와 대조한다. [하네스][harness]

## 4. 어떤 자료를 얼마나 보유했는가

다음 표의 단위는 모두 **byte이며 자료구조 용량 계산값**이다.

- **참조 eager owner**: B2B off 참조 그래프를 필터링하기 전에 완전히 전개한 `Vec<CorePostProcessExecution>`의 backing capacity와 각 실행의 nested retained bytes 합이다. JSON 키는 `reference_eager_owner_logical_bytes`다. 하네스 내부의 `avoided_eager_owner_bytes` 변수는 이 값을 보관한다.
- **최종 Core**: App 완료 응답의 `CoreExecutionResult::checked_resource_retained_bytes()` 값이다.
- **페이지 소스**: `PcReplayPageSource::checked_retained_capacity_bytes()` 값이다. 그래프와 sparse manifest, 보유한 첫 페이지 등의 해당 함수가 계산하는 용량을 뜻한다.

| Spin 정책 | 공급 | 참조 eager owner의 논리 용량 | 최종 Core의 논리 용량 | 페이지 소스의 논리 용량 |
|---|---|---:|---:|---:|
| TSpins | I | 4,180 | 35,099 | 1,995 |
| TSpins | OO | 128,096 | 35,277 | 29,386 |
| TSpins | II | 16,012 | 32,713 | 602 |
| TSpins | [OI]O | 256,160 | 35,293 | 57,315 |
| TSpins | IIOOO | 46,288,896 | 39,730 | 220,065 |
| AllMiniPlus | I | 4,180 | 35,123 | 1,995 |
| AllMiniPlus | OO | 128,096 | 35,301 | 29,386 |
| AllMiniPlus | II | 16,012 | 32,719 | 602 |
| AllMiniPlus | [OI]O | 256,160 | 35,317 | 57,315 |
| AllMiniPlus | IIOOO | 46,288,896 | 39,754 | 221,681 |

출처: [공개 검증 결과][results], 용량 계산 정의: [하네스의 `retained_eager_bytes`][harness]와 [페이지 소스 구현][page-source].

**참조 eager owner 열은 이전 B2B on App 실행에서 실제 제거된 할당량을 측정한 값이 아니다.** 기준은 의도적으로 B2B off 그래프를 먼저 만든다. 특히 `II`에서 참조 측 필터 전 4개 재생이 차지한 용량을 기존 제품의 실측 절감량으로 읽으면 안 된다. 피검증 App은 B2B on 상태에서 조기 가지치기를 적용하므로, 옛 eager 경로가 전개했을 그래프와 참조의 비제약 그래프가 다를 수 있다.

또한 표의 두 최종 owner를 더한 값만으로 전체 App의 메모리를 산출할 수 없다. 요청, 출력 payload, 페이지 방문용 임시 자료, 원래 Core와 페이지 그래프가 동시에 살아 있는 구성 구간, allocator의 보유 페이지 등은 별도의 범위다. 이 하네스 자체도 기준 replay와 실제 결과를 비교하기 위해 추가 자료를 보유한다. 표는 단계별 owner를 식별하는 근거이며 프로세스 최고치 비교로 사용하지 않는다.

### 소유권과 수명주기

| 구간 | 살아 있는 주요 자료 | 변경 또는 확인한 수명 |
|---|---|---|
| Core 실행 완료 | 입력 universe와 exact scoring graph, coverage 및 결과 메타데이터 | Path의 전체 eager replay 벡터가 비어 있음을 기준 실행에서 확인 |
| B2B 후처리 | 입력 그래프, 필터된 그래프, coverage와 audit용 중간 자료 | 완전한 그래프를 가진 paged Path이고 Save가 아닐 때 불필요한 사전 전체 replay 전개를 생략. 기존 취소·보유 예산 확인과 필터링은 수행 |
| 페이지 manifest 구성 | 원래 Core, 페이지용 그래프 owner, geometry/pattern별 manifest, 현재 셀의 replay-language 계산 상태 | compact 그래프를 페이지용으로 복사하는 동안의 중첩 수명은 남아 있다. 완료된 셀의 language 계산 상태는 버리고 sparse manifest를 보유 |
| App 완료 응답 | 결과 메타데이터, typed family, 페이지 소스와 첫 공개 witness들 | 10개 사례 모두 최종 Core의 eager replay 수 0 및 scoring batch 수 0 확인 |
| 응답 해제 후 | 하네스가 따로 잡은 페이지 소스 `Arc` | 응답을 `drop`한 뒤 모든 페이지를 다시 읽어 이전 결과와 완전히 같음을 확인 |
| 마지막 강한 참조 해제 후 | 관찰용 `Weak`만 남음 | `Weak::upgrade()`가 `None`임을 확인. 이 실행에서 App이 페이지 소스를 숨은 강한 참조로 붙잡고 있지 않음 |

구현 근거는 [App 후처리][app-services], [페이지 소스][page-source], [compact 그래프 소유 구현][source-batches]이며 마지막 세 항목은 [하네스][harness]에서 실행했다. 마지막 강한 참조 해제 검사는 해당 owner의 수명 종료를 확인한다. allocator가 물리 메모리를 OS에 즉시 반환했다는 검사나 전체 프로그램의 누수 부재 증명은 아니다. `Weak` 자체의 제어 블록 수명도 강한 참조의 수명과 구별한다.

## 5. 원시 build count와 정확한 replay count의 권한 분리

B2B 후처리는 후보·패턴의 존재 여부를 집계한다. 이 단계는 기존 원시 build 경로 multiplicity를 정확히 다시 계산했다고 주장하지 않으므로 `build_variant_count=0`, `build_variant_count_exact=false`로 남긴다. 반면 페이지 소스는 geometry와 원래 pattern별 canonical replay 언어를 검증하고 중복을 제거한 정확한 witness 수를 가진다. 두 수치는 일반적으로 같은 의미가 아니다. [B2B 후처리][b2b-postprocess] · [재생 언어 구현][replay-language]

기존 `pc.path` 검증기는 페이지 소스가 독립적인 정확한 수를 제공하는 경우에도 원시 build count의 `exact=true`를 요구해 허용된 B2B Path 요청을 실패시켰다. 최종 구현은 다음 근거를 구별한다. [Path 검증기][path-validator]

1. 공통으로 요청의 All/CountAll/Trace 계약, 탐색 완료, count 완료, objective 완료, 자원 중단 여부, 원래 패턴 수를 검증한다.
2. 완료된 페이지 소스가 있으면 problem ID, materialized pattern count, 실제 결과 그래프와의 결합을 검사한 뒤 그 소스의 정확한 witness 수를 사용한다. 미완료 세션은 `complete()`로 소스를 발행할 수 없다.
3. 페이지 소스가 없는 eager 경로는 기존 원시 `build_variant_count_exact=true`와 완전한 replay batch를 계속 요구한다.

공개 하네스는 **10개 사례 전부에서 raw count가 `0/false`로 유지되는 동시에 typed family의 exact witness count가 기준과 일치함**을 확인했다. 결과의 `path_count_authority`는 `completed-bound-canonical-replay-manifest`다. 알 수 없는 원시 수치를 알려진 값으로 덮어쓰거나 완전성 검사를 없애서 통과시킨 것이 아니다. [공개 검증 결과][results]

## 6. 실제 실행한 실패 처리와 아직 실행하지 못한 검증

두 spin 정책의 `I` 사례마다 다음 세 검사를 실제 실행했다. [하네스][harness] · [공개 검증 결과][results]

| 조건 | 확인한 결과 |
|---|---|
| 실행 전 이미 취소된 token | `ExecutionFailed`, 취소 오류, product capability 결과 없음 |
| App 제품 보유 예산 1 byte | `ExecutionFailed`, memory/limit 오류, product capability 결과 없음 |
| 공개 manifest 생성 예산 1 byte | `PcReplaySourceBuildSession::new`가 오류 반환 |

이 검사는 과소 예산 및 선행 취소에 대해 불완전한 성공 결과를 내보내지 않음을 보여준다. 실행 중 임의 지점의 취소, 실제 OS 메모리 압박 또는 OOM 상황 전체를 이 하네스에서 재현한 것은 아니다.

신규 private 테스트 4개는 `cargo check --locked -p clearra-app ... --lib --tests`에서 타입 검사를 통과했다. 검사 범위에는 eager/lazy 전체 witness 대조, 직접 App 최종화, raw count와 완료 manifest 권한 분리, 완료 필드·패턴 수·그래프 불일치 및 미완료 manifest/batch 거절, 기존 eager/Save 경계가 들어 있다. **이 4개의 test body를 실행했다는 뜻은 아니다.** [private 테스트 소스][private-tests] · [타입 검사 로그][typecheck-log] · [타입 검사 영수증][typecheck-receipt]

App 전체 libtest 실행 파일 생성은 같은 구성에서 두 번 `rustc`의 SIGKILL로 끝났고 해당 로그를 보존했다. 두 runtime 영수증은 Cargo 종료 코드 101, supervisor 강제 종료 사유 없음, 관찰한 메모리 압박 이벤트 0, 프로세스 트리 종료 및 잔존 자식 0을 기록한다. cgroup OOM-kill 증분은 `null`로 확인되지 않았다. 따라서 원인을 확정된 OOM이라고 표현하지 않는다. 공개 API 검증과 타입 검사 통과를 App 전체 private suite 실행 성공으로 대체해서 보고하지 않는다. [첫 SIGKILL 로그][sigkill-log-1] · [동일 구성 재확인 로그][sigkill-log-2] · [첫 영수증][sigkill-receipt-1] · [두 번째 영수증][sigkill-receipt-2]

하네스 초기 실패도 보존했다. 첫 실패는 비어 있는 deferred `postprocess_pattern_weights`를 원래 universe로 오인한 하네스 가정이었다. 두 번째 실패는 실제 제품의 `pc path build-variant count is inexact` 계약 오류였으며, 위 권한 분리 수정 후 v3의 10개 실행이 통과했다. [첫 실패][v1-log] · [제품 계약 실패][v2-log] · [v3 결과][results]

## 7. 협력적 GC가 보장하는 범위

관리자가 직접 실행하는 Node 루트에는 `--expose-gc`와 responder를 연결한다. responder는 `isMainThread`, `threadId === 0`, 요청의 PID와 request ID 등을 확인한 뒤 자신의 `global.gc()` 호출이 끝난 경우에만 `scope: node-main-isolate` ACK를 쓴다. 관리자는 protocol, request ID, action, completed 상태, child PID가 일치하는 ACK를 확인한다. [Node responder][node-gc] · [관리자 런타임][manager-runtime]

파일 감시가 시작할 때 또는 이후에 실패해도 1초 polling이 요청을 확인한다. watcher와 timer는 `unref()`되어 작업 수명을 불필요하게 연장하지 않는다. 기존 실행 로그의 5개 테스트는 루트 ACK, 같은 PID의 worker isolate ACK 차단, 감시기 생성·동작 중 실패 후 polling, GC가 공개되지 않은 루트의 ACK 금지를 모두 통과했다. [GC 검증 로그][gc-log]

이 회수 정책에는 다음 경계가 남아 있다.

- Node main event loop가 실행 기회를 주어야 responder가 동작한다. 긴 동기 연산 중에는 즉시 회수가 보장되지 않는다.
- 루트 main isolate의 ACK는 다른 worker isolate, 자식 프로세스, 브라우저 renderer, Rust owner 또는 WASM linear memory의 회수를 증명하지 않는다.
- Rust의 `Arc`/`Vec` 수명은 소유권 해제로 다룬다. 이 App 하네스는 Rust 실행 파일이므로 runtime 영수증에 협력적 GC unavailable 및 요청·ACK 각각 0이 기록되었다. 이번 App 검증은 GC로 회수한 사례가 아니다.
- 관리자는 압박 요청과 완료 ACK를 구별한다. 심각한 압박 또는 소유 메모리 한도 문제가 해소되지 않으면 소유 프로세스 트리만 종료하며, 요청을 썼다는 이유로 회수를 완료했다고 기록하지 않는다. 자동 자원 변경 재시도도 하지 않는다.

이는 [responder][node-gc], [관리자 런타임][manager-runtime], [App 실행 영수증][run-receipt]에서 확인한 범위다. Node GC 기능을 제품 전체의 OOM 방지 완료로 확대 해석하지 않는다.

## 8. 결과를 적용할 때의 제한

이 검토가 뒷받침하는 것은 작은 고정 보드의 공개 B2B Path 정확성, 해당 eager owner 생략, 최종 Core 생산자 자료 정리, 페이지 소스의 독립 수명 및 마지막 강한 참조 해제다. P7/P7P4의 전체 옵션 성능, Auto 작업자 구성의 속도, 실제 GUI 또는 Discord 수명, 프로세스 최고 메모리 변화는 각각의 별도 측정 근거로 판단해야 한다. Full 6L은 이 검증에 포함하지 않았다.

문서 작성에서는 기존 소스와 JSON/로그/영수증을 읽었고 생산 소스·HEAD를 수정하거나 추가 빌드·탐색·테스트를 실행하지 않았다.

[results]: ../../_local/artifacts/gui-option-optimization-20261124/app-public-validation-v3.json
[run-log]: ../../_local/artifacts/gui-option-optimization-20261124/app-public-validation-v3.log
[harness]: ../../_local/artifacts/gui-option-optimization-20261124/app_public_validation.rs
[link-artifacts]: ../../_local/artifacts/gui-option-optimization-20261124/app-public-link-artifacts-v3.json
[run-receipt]: ../../_local/artifacts/gui-option-optimization-20261124/receipts/1795494308792316924-5-runtime.json
[typecheck-log]: ../../_local/artifacts/gui-option-optimization-20261124/app-test-typecheck.log
[typecheck-receipt]: ../../_local/artifacts/gui-option-optimization-20261124/receipts/1795494348608323833-5-runtime.json
[v1-log]: ../../_local/artifacts/gui-option-optimization-20261124/app-public-validation.log
[v2-log]: ../../_local/artifacts/gui-option-optimization-20261124/app-public-validation-v2.log
[sigkill-log-1]: ../../_local/artifacts/gui-option-optimization-20261124/integrated-regressions-final.log
[sigkill-log-2]: ../../_local/artifacts/gui-option-optimization-20261124/integrated-same-config-recheck.log
[sigkill-receipt-1]: ../../_local/artifacts/gui-option-optimization-20261124/receipts/1795491704638535611-5-runtime.json
[sigkill-receipt-2]: ../../_local/artifacts/gui-option-optimization-20261124/receipts/1795491932971027607-5-runtime.json
[app-services]: ../../crates/clearra-app/src/app_services.rs
[page-source]: ../../crates/clearra-app/src/pc_replay_page_source.rs
[source-batches]: ../../crates/clearra-app/src/pc_replay_source_batches.rs
[path-validator]: ../../crates/clearra-app/src/pc_path_result.rs
[b2b-postprocess]: ../../crates/clearra-app/src/execution_constraint_postprocess.rs
[replay-language]: ../../crates/clearra-postprocess/src/score_batch/exact_replay_language.rs
[private-tests]: ../../crates/clearra-app/src/pc_path_deferred_postprocess_tests.rs
[node-gc]: ../../scripts/runtime/clearra-node-gc.cjs
[manager-runtime]: ../../tools/clearra-manage/src/runtime.rs
[gc-log]: ../../_local/artifacts/gui-option-optimization-20261124/pinned-node-gc-tests.log

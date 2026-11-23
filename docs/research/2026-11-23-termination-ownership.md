# 내부 강제 종료와 프로세스 소유권 감사 — 2026-11-23

## 1. 판정과 범위

활성 Rust manager의 Unix 정리 경로에 실제 소유권 결함이 있었다. 직접 자식의 종료와
프로세스 그룹의 소멸을 확인한 뒤에도 `Containment::drop`이 저장해 둔 숫자 PGID에
SIGKILL을 다시 요청했다. 종료 상태를 이미 수집한 직접 자식에 대한 별도 `Child::kill`
요청도 중복될 수 있었다. 이번 변경은 확인된 소멸 이후의 권한을 폐기하고, 상태가
불명확하거나 아직 살아 있는 소유 트리에 대한 비상 정리는 유지한다.

별도 릴리스 명령 래퍼에서도 ESRCH를 받은 뒤 후속 타이머가 같은 PGID에 다시 신호를
보낼 수 있었다. 이 경로에도 동일한 소유권 수명 규칙을 적용했다. 릴리스 래퍼는 현재
native `symbolic_family_compare` 실행 경로에 포함되지 않는다.

**이 결함을 고쳤다는 사실은 이전 P7P4 SIGKILL의 원인을 확인했다는 뜻이 아니다.**
감사가 추가된 세 실행에서는 manager가 자식의 종료를 관측하기 전 보낸 종료 요청이
없었고, 유일한 SIGKILL 요청은 그 뒤 실패한 Drop 정리였다. 시간 순서상 그 요청은
앞선 자식 종료를 일으킬 수 없다. 커널 OOM과 외부 사용자 공간 발신자를 구분할
호스트 증거는 확보되지 않았다.

감사는 시작 커밋 `a4f264363e80334154cbf0c39e2c74f5c4d3ac08`의 활성 소스와 이번 좁은
변경을 대상으로 했다. C/C++·헤더·FFI, Rust, JS/TS, 셸, PowerShell, Windows 프로세스 API, Worker
수명 API, 숫자 signal 9 및 `-9`, `kill`/`abort`/watchdog 동의어를 검색하고 실제
호출 지점을 읽었다. 역사적 Python 구현, 회귀 fixture, 문서와 workflow의 정적
내용은 별도로 분류했다. 외부 라이브러리나 호스트의 모든 종료 경로를 증명하는
감사는 아니다. CI 조회·실행·대기, 배포 및 역사적 Python 실행은 하지 않았다.

## 2. 관측된 종료와 내부 요청의 구별

아래 세 실행은 이전 [빌드·signal 진단 보고서](build-and-signal-diagnostics-2026-11-23.md)의
원시 receipt와 stderr를 근거로 한다. `attempts=[]`만 단독으로 읽지 않고 Drop 이전
snapshot 경계, stderr의 후속 event, `unretained_attempts=0`을 함께 확인했다.

| 실행 | receipt ID | manager 경과 시간 | wait 이전 내부 종료 요청 | wait 이후 기록 |
| --- | --- | ---: | --- | --- |
| compatibility filter | `1795425735355417354-3` | 34.771 s | 없음 | Drop의 SIGKILL, ESRCH, 실패 |
| row-prefix filter | `1795426382130762307-3` | 34.755 s | 없음 | Drop의 SIGKILL, ESRCH, 실패 |
| piece cache 수명 변경 | `1795427063731448912-3` | 44.694 s | 없음 | Drop의 SIGKILL, ESRCH, 실패 |

세 receipt 모두 raw child termination은 Unix signal 9, 반환 코드는 137이었다.
`supervisor_termination_reason=null`, timeout 0, pressure events 0,
`automatic_retry=false`이며, 종료 요청 event에는
`child_wait_status_observed=true`, `succeeded=false`, `os_error_code=3`이 기록됐다.
이 표의 시간은 완료 시간이나 같은 작업량의 성능 비교가 아니다.

원시 자료는 `_local/artifacts/full4-symbolic-{compatibility,prefix,piece-cache}-20261123`
접두사의 `.runtime.json`, `.observation.json`, `.log`다. 처음 두 legacy/symbolic 실행의
receipt에는 이번 수준의 요청 audit가 없으므로 이 세 실행의 계측을 소급 적용하지
않는다. 그 이전 결과는 [Full4 비교 보고서](full4-symbolic-family-comparison-2026-11-23.md)에
유지한다.

해석에 사용하는 OS 경계는 다음과 같다.

- `kill(-pgid, signal)`은 프로세스 그룹을 대상으로 한다. 음수 대상은 음수 signal이
  아니다. signal 0은 존재·권한 조회이며 종료 신호를 보내지 않는다. ESRCH는 해당
  프로세스 또는 그룹을 찾지 못했다는 결과다.
- Rust `Child::kill`은 Unix에서 SIGKILL에 해당한다. `try_wait`는 종료를 관측하면
  Unix 자식을 reap하며, 이후에도 수집한 종료 상태를 반환한다. 이미 수집한 상태는
  직접 자식에 대한 재발신이 필요 없다는 근거가 된다.
- `abort()`라는 이름만으로 OS 종료라고 분류하지 않는다. 아래의 executor queue
  abort, 파일 transaction abort, AbortController, Worker 종료는 각각 별도 소유권과
  실행 단위를 가진다.

OS 의미의 근거: [Linux kill(2)](https://man7.org/linux/man-pages/man2/kill.2.html),
[Linux wait(2)](https://man7.org/linux/man-pages/man2/waitpid.2.html),
[Rust Child](https://doc.rust-lang.org/std/process/struct.Child.html).

## 3. 활성 종료 지점 목록

### 3.1 직접 native benchmark에 도달할 수 있는 경로

현재 직접 실행 경계는 Rust manager → `symbolic_family_compare`이다. benchmark
바이너리는 `tools/clearra-symbolic-bench`의 일반 dependency를 통해 executor를
사용한다. Discord job runner, release command wrapper, Chromium, PowerShell,
WSL session 및 역사적 Python wrapper를 거쳐 실행하는 경로가 아니다.

| 파일·호출 단위 | 실제 동작과 발동 조건 | 이번 판정 |
| --- | --- | --- |
| [`runtime.rs`](../../tools/clearra-manage/src/runtime.rs): `TerminationAudit::kill_child` | attach 실패나 escalation fallback에서 소유한 직접 자식을 강제 종료한다. | 이미 wait 상태가 있거나 `try_wait`로 얻으면 요청 자체를 생략한다. 불명 상태의 비상 fallback은 유지한다. |
| 같은 파일: Unix `request_termination` | 명시적 취소, owner EOF, 활성화된 deadline, 출력·프로세스·메모리 정책 실패에 대해 소유 그룹에 TERM을 요청한다. | 살아 있거나 불명인 그룹에 유지한다. ESRCH이면 해당 숫자 PGID의 권한을 폐기한다. |
| 같은 파일: Unix `force_terminate` | grace 이후 KILL, 직접 자식 종료 후 남은 descendants 정리, 예외 scope의 Drop 정리다. | 정상 종료·빈 그룹 확인 이후의 재발신을 제거한다. 살아 있는 descendants 및 비상 정리는 유지한다. |
| 같은 파일: Windows `TerminateJobObject`, `CloseHandle` | 소유 Job의 전체 트리 종료와 kill-on-close이다. | 숫자 PGID와 달리 소유 kernel handle이므로 닫기 의무를 유지한다. Windows 동작을 Unix식으로 삭제하지 않는다. |
| 같은 파일: `process_group_usage`, `pid_alive` | Linux `/proc` 조회 또는 signal 0 / Windows process handle 존재 조회다. | 종료 요청이 아니다. |
| [`symbolic_family_compare.rs`](../../crates/clearra-core-executor/examples/symbolic_family_compare.rs) 및 다른 symbolic example의 `main` | 제품 stack 크기의 작업 thread를 join한 뒤 명시적 결과 코드로 `process::exit`한다. | 직접 SIGKILL 호출은 없다. 오류 반환과 raw Unix signal 종료를 혼동하지 않는다. |

manager 정책 분기에서는 `forced_reason`을 설정한 뒤 `terminate_tree`로 들어간다.
관측된 세 P7P4 receipt의 `forced_reason` 부재와 요청 audit를 이 제어 흐름에 함께
대조했다. 출력 reader의 1초/2초 표본 주기나 내부 polling은 solver 종료 deadline이
아니다. solver의 hot path에 supervisor 로직을 추가하지 않았다.

### 3.2 별도 native·JS 실행 경로

| 소스 | 종료 대상·소유권 | 직접 P7P4 경로와의 관계 |
| --- | --- | --- |
| `crates/clearra-cli/src/accelerator_download_transport.rs`, `tablebase_download_transport.rs`, `tablebase_curl_batch.rs` | 자신이 시작한 curl `Child`를 취소·전송 오류·입력 bound·Drop에서 kill한 뒤 wait한다. 일부 오류에서 kill을 재시도하지만 wait 이전의 동일 Child이며, 확인된 reap 이후 저장 PGID를 재발신하는 구조가 아니다. | 다운로드 명령 경로다. 현재 benchmark에서 호출하지 않는다. reader 종료 전에 필요한 kill을 일괄 삭제하지 않았다. |
| `crates/clearra-render/src/bin/clearra_asset_import.rs::wait_for_worker` | 자신이 시작한 SVG import worker의 명시적 제한에 따른 kill/wait다. | 별도 asset import 도구다. |
| [`apps/clearra-discord-bot/src/job-service/runner.mjs`](../../apps/clearra-discord-bot/src/job-service/runner.mjs) | 취소·Cloud Run 경계·출력 실패에서 활성 Child에 TERM을 보내고, 여전히 살아 있으면 grace 뒤 KILL한다. finish가 타이머를 정리하고 실패 수락 뒤 늦은 성공을 막는다. | 별도 서비스 wrapper다. SIGKILL 문자열을 비교하는 OOM 분류 부분은 신호 발신이 아니다. |
| [`scripts/tools/invoke-clearra-build.mjs`](../../scripts/tools/invoke-clearra-build.mjs) | 자신이 만든 detached group의 활성 child에만 취소·build-storage 실패 신호를 전달한다. 이미 runtime supervisor 안에 있으면 새 그룹을 만들지 않는다. exit 뒤에는 signal 0으로 drain을 확인한다. | 빌드 소유자다. 현재 solver 실행 경계가 아니다. |
| [`scripts/tools/managed-search-owner.mjs`](../../scripts/tools/managed-search-owner.mjs) | owner 취소 시 manager stdin lease를 닫는다. manager를 직접 kill하거나 unref하지 않는다. | managed search를 시작하는 wrapper의 취소 계약이다. receipt를 남길 manager의 수명을 보존한다. |
| [`scripts/release/bounded-command.mjs`](../../scripts/release/bounded-command.mjs) | 명시적 외부 명령 제한·출력 bound·취소에서 POSIX owned group TERM→KILL→bounded stream drain, Windows exact PID tree의 taskkill을 사용한다. | 릴리스 관측·evidence 수집 도구다. ESRCH 이후 재발신만 이번에 제거한다. |
| `scripts/benchmark/run-v081-accelerator-abba.mjs` | 직접 child에 선택적 진단 deadline/출력 overflow 종료를 요청한다. 별도 KILL은 종료 stop-file 이후에도 남은 Windows 메모리 probe helper에 대한 것이다. | 다른 명시적 benchmark harness다. 이번 symbolic 하네스에 삽입되지 않는다. |
| `scripts/tools/run-wasm-browser-benchmark.mjs` | stop 요청 뒤 남은 probe helper와 CDP Browser.close 뒤 남은 소유 Chromium child를 정리한다. | 브라우저 benchmark 도구다. |
| `scripts/tools/run-gui-experiment.mjs`, `build-clearra-frontend.mjs`, `scripts/ci/run-independent-checks.mjs` | 소유 child에 IPC 종료/부모 취소 신호를 전달한다. GUI wrapper는 IPC 종료 뒤 grace fallback을 가진다. | 개발·빌드·정적 확인 경로다. CI 소스만 읽었으며 CI를 실행하거나 조회하지 않았다. |

### 3.3 셸·Windows·Worker·fixture 경계

| 범주 | 소스·동작 | 유지 이유와 경계 |
| --- | --- | --- |
| WSL per-request unit | `scripts/runtime/clearra-wsl-session.sh`의 exact unit stop, `KillMode=control-group`, `TimeoutStopSec`, `OOMPolicy=kill` 및 systemctl helper의 `timeout --kill-after` | lease를 잃은 요청의 unit만 종료한다. distribution 전체 종료가 아니다. `tools/clearra-manage/src/wsl.rs`의 `--shutdown` 문자열은 거부 fixture 입력이다. 직접 native 실행은 WSL wrapper를 사용하지 않는다. |
| Windows 진단·테스트 helper | `scripts/lib/progress/native_progress_runner.ps1`, `scripts/lib/rust-exact-tests.ps1`, `scripts/tools/verify-windows-desktop-runtime.ps1`, `scripts/benchmark/read-ci-preview-wasm.ps1` | 소유한 Process 객체, GUI handle 또는 과대 다운로드 child를 종료하고 기다린다. 전역 프로세스 이름 기반 kill을 추가하지 않았다. |
| 릴리스·배포 관리 | `apps/clearra-discord-bot/scripts/{produce-oracle-deployment-proof,observe-oracle-candidate}.mjs`, extensionless `run-oracle-bounded-job-probe`, `scripts/release/oracle/invoke-release-deploy-v080.ps1`, `scripts/release/oracle/clearra-oracle-release-deploy-v080`, `apps/clearra-discord-bot/scripts/restore-oracle-release` | 외부 관측 helper의 timeout/kill, exact service stop 및 exact SSH Process 종료다. 배포·rollback 작업의 제어 경계이며 solver 자체의 deadline이 아니다. 아무 배포도 실행하지 않았다. |
| 정적 workflow wrapper | `.github/actions/upload-production-evidence/action.yml`, `.github/workflows/{candidate-preflight,discord-deploy}.yml`, `scripts/release/restore-discord-catalog-v080.sh` | 외부 CI/업로드/관측 명령을 제한하는 `timeout --kill-after`다. 정적 소스 분류만 했으며 workflow 조회·dispatch·poll은 하지 않았다. |
| Browser/Node Worker | `packages/clearra-ui/src/lib/wasm/wasmWorkerLifecycle.ts`, workspace reader/export/interchange, `packages/ctk3/src/asyncCodec.ts`, `apps/clearra-web/src/workers/ClearraVerifierPool.ts`, `apps/clearra-discord-bot/src/viewer/async-gif.mjs` 및 브라우저 시험 helper | 정확한 Worker 객체의 수명을 끝낸다. standalone native 프로세스에 숫자 PID/PGID SIGKILL을 보내는 경로가 아니다. |
| 중단 이름의 비신호 API | executor `parallel_worker`, `setup_parallel`, `queue_observation_shared_producer`; 파일 atomic publication; JS AbortController | queue 상태·조건 변수·transaction·요청 취소를 변경한다. 이름이 `abort`여도 `process::abort`가 아니다. |
| 강제 종료 시험 | `crates/clearra-platform-fs/src/windows.rs`의 cfg-test 및 `tests/native_atomic_publication.rs`; `crates/clearra-wasm/tests/pc_minimals_ctk3_stage_probe.rs`; `scripts/tools/powershell-test-process.mjs`; 각 lifecycle fixture | 원자적 게시의 crash fixture, 별도 probe 시험 deadline, 테스트 child 회수다. 새 `owner-lifetime-fixture.mjs::self-signal`은 신호와 supervisor reason을 분리해 검사하기 위해 자기 자신에게 SIGKILL을 보내는 시험 전용 코드다. |

기존 WASM stage probe에는 30초·240초 등을 갖는 fixture child가 남아 있다. 이것을
제품 search deadline으로 옮기지 않았고, 해당 fixture를 이번 Full4 실행에 사용하지도
않았다. UI progress watchdog와 HTTP AbortController는 관측·취소 대상을 별도로
가진다. 원시 native signal 9를 설명하는 경로로 합쳐 서술하지 않는다.

역사적 `scripts/management/history/python-v1/{clearra_runtime,clearra_process_wrapper}.py`에는
`os.kill`, `os.killpg`, SIGTERM/SIGKILL, Windows Job 종료가 있다. 이는 활성 Rust manager와
분리된 역사 소스다. 현재 실행 entrypoint로 사용하지 않았고, 활성 entrypoint의 해당
history 경로 호출도 검색에서 발견되지 않았다. 역사 문서의 signal 관측은 당시 증거로
보존하며 새 runtime 결과로 덮어쓰지 않는다.

### 3.4 추적 중인 C/C++·헤더와 FFI 추가 확인

디렉터리별 검색에서 `core-c`가 빠지지 않도록 `git ls-files`를 기준으로 C/C++,
대소문자 C/header, `.cc/.cpp/.cxx/.c++`, `.hh/.hpp/.hxx/.inc`, Objective-C,
assembly와 Zig 확장자를 다시 열거했다. 해당 추적 파일은 **285개이며 실제로 모두
`core-c` 아래의 `.c/.h`**였다. public header, production source, tests, benchmark
tool을 포함해 검사했으며, 이 목록에 first-party C++ 또는 assembly 파일은 없었다.

`core-c` 전체에서 `kill`, `killpg`, `tkill`, `tgkill`, `pthread_kill`, `raise`,
`abort`, `syscall`, `SYS_*`/`__NR_*` signal 번호, `pidfd_send_signal`, SIGKILL,
trap/fastfail와 shell process 실행에 해당하는 명시적 종료 지점은 발견되지 않았다.
모든 추적 Rust 소스에서도 raw syscall, inline/global assembly, 종료 함수를 우회하는
`link_name`을 함께 검색했다. 기존 manager와 앞서 분류한 owned Child 종료 및
시험 전용 abort 외에 추가 신호 지점은 발견되지 않았다. JS/셸 등의 `pkill`,
`killall`, 숫자 `-9` 및 숫자 signal 9 전달도 같은 추가 검사에 포함했다.

FFI의 이름만 보고 신호라고 오인할 수 있는 경계는 직접 읽어 확인했다.

| 소스 | 실제 의미 |
| --- | --- |
| [`core-c/src/execution/execution_control.c`](../../core-c/src/execution/execution_control.c) | thread-local control에 설치한 atomic cancellation flag를 주기적으로 읽고 bool을 반환한다. OS 프로세스 신호를 보내지 않는다. |
| [`clearra-core-ffi/src/memory/release_signal.rs`](../../crates/clearra-core-ffi/src/memory/release_signal.rs) | `Rc<RefCell<...>>` 안의 context/search/batch 해제 횟수를 기록한다. 이름의 Signal은 OS signal이 아니다. |
| [`clearra-core-ffi/src/raw/native_thread_affinity_linux.rs`](../../crates/clearra-core-ffi/src/raw/native_thread_affinity_linux.rs) | `sched_getaffinity`/`sched_setaffinity`와 CPU bitset으로 현재 worker의 affinity를 읽거나 설정한다. PID 0은 여기서 호출 thread를 뜻하며 kill 호출이 아니다. |

현재 symbolic 도구의 기본 feature는 `native-c-core`를 활성화하지 않는다. 그래도
제품의 다른 native C 경로를 감사에서 제외하지 않고 위 285파일을 확인했다.
이는 first-party의 명시적 종료 호출에 대한 소스 감사이며, third-party 전체나
호스트에서 발생할 수 있는 모든 종료 동작의 부재를 증명한다는 주장은 하지 않는다.

## 4. 적용한 소유권 수정

### Rust manager

Unix `Containment`는 `cleanup_armed: Cell<bool>`로 자신이 만든 그룹에 대한 정리
권한의 수명을 표현한다. 상태 전이는 다음과 같다.

| 관측 | 상태 전이 | 허용 동작 |
| --- | --- | --- |
| attach 성공 | armed | TERM, 필요한 KILL, 예외 Drop 정리 가능 |
| 직접 자식 종료 + group count `Some(0)` | retired | 이후 숫자 PGID에 신호를 보내지 않음 |
| 그룹 signal 호출 결과 ESRCH | retired | 이후 escalation·Drop의 같은 PGID 재사용 금지 |
| child 미종료, group 존재 또는 group 상태 `None` | armed 유지 | 소유 트리의 종료와 비상 정리 유지 |

`confirm_tree_stopped`는 정상 wait 뒤, grace polling 중, force escalation 직전에 같은
조건을 사용한다. `TerminationAudit::poll_child_exit`는 이미 수집한 상태를 재사용하거나
safe `Child::try_wait`로 확인하며, 오류를 종료 증거로 취급하지 않는다. `kill_child`는
관측된 자식에 대한 중복 요청을 audit에 만들지 않는다. 그룹 조회가 불명확하거나
직접 자식이 원래 그룹을 벗어난 경우를 위해 미종료 Child fallback 자체는 유지한다.

Windows의 소유 Job handle은 계속 닫는다. 마지막 handle을 닫는 기존 containment
계약과 attach 실패 cleanup은 그대로다. 새 unsafe, 새 dependency, 새로운 syscall
경계, solver 내부 supervisor hook은 추가하지 않았다.

### 릴리스 외부 명령 wrapper

`groupCleanupArmed`는 POSIX signal 결과가 ESRCH일 때만 false가 된다. 살아 있는
그룹, EPERM 등 불명 상태에는 TERM/KILL escalation과 출력 pipe drain을 유지한다.
Windows taskkill 흐름과 기존 timeout/출력 bound를 변경하지 않았다. 이 변경은
native benchmark 코드나 생성·탐색 알고리즘에 영향을 주지 않는다.

### 남아 있는 수명 한계

이번 수정은 **leader를 reap한 순간부터 첫 group 관측·정리까지의 PID/PGID 재사용
경쟁을 완전히 닫지 않는다.** 이 구간에는 숫자 PGID만 남으며, 프로세스 그룹 조회와
다음 signal 호출도 원자적 연산이 아니다. 이미 소멸을 확인한 뒤의 반복 요청을 제거한
것과 최초 조회·신호의 안정된 identity를 보장하는 것은 서로 다른 조건이다.

전체 경계를 닫으려면 예를 들어 wait 관측 중 leader를 unreaped 상태로 보유하거나
별도의 안정된 OS containment capability가 필요하다. `waitid(WNOWAIT)`는 상태를
소비하지 않는 관측을 제공하지만, 현재 manager에 그 안전한 소유권 경계를 새로 넣지
않았다. 이번 범위는 새 unsafe 없이 narrow disarm을 적용하는 것으로 정했다.
비 Linux Unix의 signal-0 probe, Linux `/proc` 가시성, 그룹을 벗어나는 descendant를
다루는 기존 한계도 이번 변경으로 사라졌다고 주장하지 않는다.

## 5. 실제 검증

루트 agent가 아래 시험과 정상 manager 재빌드를 순서대로 수행했다. 이 문서를 작성한
agent는 별도 Cargo나 시험 실행을 겹치지 않았다.

| 검증 | 결과 | 근거 |
| --- | --- | --- |
| `cargo test --locked -p clearra-manage termination_ -- --test-threads=1` | 5/5 PASS | `_local/artifacts/termination-owner-tests-20261123.log`, receipt `1795428696270196700-3` |
| `cargo build --locked -p clearra-manage` | PASS, 1.49 s | `_local/artifacts/termination-owner-manager-build-20261123.log`, receipt `1795428716978642532-3` |
| `node --test tools/clearra-manage/tests/owner-lifetime.test.mjs` | 5/5 PASS | `_local/artifacts/termination-owner-lifecycle-20261123.log` |
| `node --test scripts/release/bounded-command.test.mjs` | 3/3 PASS | `_local/artifacts/termination-release-helper-tests-20261123.log` |
| 변경 Rust의 rustfmt 및 변경 diff 공백 검사 | PASS | 소스 변경 뒤 확인, 새로운 unsafe 없음 |

Rust filter의 5개는 기존 signal-vs-supervisor 분류, audit wait 경계와 새 세 소유권
fixture다. 새 fixture는 정상 완료 그룹의 Drop 재발신 부재, 불명 상태에서의 실제
비상 SIGKILL 유지, reap된 Child의 직접 kill 생략을 검사한다.

재빌드된 정상 manager를 사용한 Node lifecycle 결과는 다음과 같다.

| 경계 | receipt ID | 관측 |
| --- | --- | --- |
| timeout 0 정상 완료 | `1795428739456842987-18` | 정상 완료, 종료 audit 비어 있음, Drop 재발신 없음 |
| child의 시험용 자기 SIGKILL | `1795428739824827191-30` | raw signal 9 / code 137, supervisor reason null, 종료 audit 비어 있음 |
| owner EOF 취소 | `1795428740033437786-42` | lease 종료 실패, 늦은 성공 없음, tree drain 확인 |
| descendant가 보유한 output pipe | `1795428740241848268-54` | 남은 descendant 정리 후 reader join, tree drain 확인 |
| 명시적 1초 fixture deadline | `1795428740552593493-74` | timeout 실패 유지, tree drain 확인 |

모든 Unix lifecycle fixture에서 확인된 정지 뒤의 `drop-cleanup` event가 없었다.
1초 제한은 이 correctness fixture의 입력이며, 제품 또는 P7P4 search deadline을
추가한 것이 아니다.

**빌드 로그를 감독한 outer manager는 재빌드 이전 바이너리였다.** 따라서 위 Cargo
시험·빌드 로그 끝에 남은 이전 버전의 post-wait ESRCH Drop event는 수정 실패를
뜻하지 않는다. Node lifecycle은 새 정상 바이너리를 선택해 별도로 검사했다.
`cargo test` 실행 파일과 일반 `debug/clearra-manage`의 갱신 여부를 혼동하지 않도록
정상 바이너리 build를 명시적으로 수행했다.

릴리스 helper 시험은 기존 실제 child의 timeout·취소 외에 두 경계를 추가했다.
짧게 실행되는 실제 child가 close를 지연하는 동안 OS ESRCH 결과를 주입해 후속
재발신 부재를 검사했고, EPERM을 주입한 다른 child에는 실제 SIGKILL escalation과
reap을 확인했다. ESRCH fixture의 부재 결과는 의도적 fault injection이며 실제
커널에서 그 child가 사라졌다는 측정으로 보고하지 않는다. 작은 JS fixture는 core
빌드 중 수행됐으나 P7P4 성능 측정과 겹치지 않도록 실행 계획을 분리했다.

## 6. 다음 실행에서 판단할 수 있는 것

새 관리자는 확인된 빈 그룹에 대한 중복 정리를 제거하므로, 이후 stderr의 종료
event는 살아 있거나 확인되지 않은 소유 트리의 실제 정리 요청에 대응해야 한다.
다음 P7P4에서도 raw child termination, 요청 event의 wait 전후 순서,
supervisor reason, tree-stopped 증거를 함께 읽어야 한다.

이 문서의 lifecycle 시험은 Full4 해법 집합의 완료, 정확성 비교, 성능 개선율 또는
Full6 진행 가능성을 대신하지 않는다. 강제 종료 호출을 모두 삭제해 containment를
약화시키지 않았으며, 이번 수정만으로 이전 signal 9가 재발하지 않는다고 보장하지
않는다. 새로운 실측 결과가 있으면 해당 실행의 보고서에서 별도로 판정한다.

## 7. 수정된 manager의 첫 P7P4 실행

소유권 수정과 정상 manager 재빌드 뒤, 동일한 empty 4L / P7P4 / Jstris180 /
Hold enabled / 10 locks / 11 draws 입력으로 실제 symbolic 실행을 진행했다.
실행은 **42.328초에 raw Unix signal 9 / code 137로 종료됐고, 탐색은 미완료**였다.
timeout, 명시적 자원 옵션, 요청 worker 정책을 줄여 재시도한 결과가 아니다.

| 구분 | 실행 지문 |
| --- | --- |
| receipt ID | `1795429158707501894-3` |
| compiled source tree | `cee6f398c04a71f280077647786735dcfcf732b9` |
| symbolic 바이너리 SHA-256 | `26b67ead8267d5a1b2509063d193dc8105a0e0cf861a09aeb954fa428cbda4d9` |
| manager 바이너리 SHA-256 | `f722726c2acc8f99101cb7cf57e4eede75d0b187e2680fe03e96a83dc1828385` |

이번에는 receipt의 `termination_audit.attempts=[]`, `unretained_attempts=0`에 더해,
전체 stderr에서 추출한 `termination_attempt_stderr_events=[]`였다. 즉 wait 이전의
정책 종료 요청뿐 아니라 이전 실행에 남았던 **wait 이후 Drop의 SIGKILL 요청도
없었다.** `supervisor_termination_reason=null`, timeout 0, pressure events 0,
`automatic_retry=false`, descendants 0, `process_tree_stopped=true`도 함께 확인했다.

따라서 확인된 빈 그룹의 후속 재발신을 제거한 수정은 실제 benchmark 경계에서도
작동했다. 그 요청이 없어도 SIGKILL이 재발했다는 사실은 소유권 결함을 이전 종료의
확정 원인으로 설명할 수 없음을 더 분명하게 한다. 이 실행에서 manager가 기록된
종료 API를 통해 자식을 강제 종료했다는 증거는 없다. cgroup OOM counter와 유한
상위 limit은 여전히 null이므로, 내부 발신자나 OOM 중 하나를 확정하는 근거로
바꿔 쓰지 않는다. 앞 절의 최초 reap→group 정리 경쟁 및 OS 가시성 한계도 유지된다.

마지막 관측은 rank 2 완료 marker 이후 rank 3의 movement 단계였다. summary의
`search_complete`, identity export, global coverage export, joint comparison은
모두 false였다. 42.328초를 완성된 해법 집합의 계산 시간이나 이전 후보 대비 개선율로
보고하지 않으며, Full6 실행 가능성의 근거로 사용하지 않는다.

직접 확인한 원시 자료는 `_local/artifacts/full4-symbolic-ownership-20261123` 접두사의
`.summary.json`, `.observation.json`, `.movement-observation.json`, `.runtime.json`이다.
첫 두 파일의 바이너리·요청 지문, runtime receipt의 종료 audit와 상태, 전체 stderr
종료 event 추출을 함께 대조했다. 별도 source 개선 후보의 결과는 이 실행과 섞지 않는다.

### 후속 두 후보의 종료 기록

같은 수정 manager를 사용한 다음 두 소스 후보도 탐색 완료 전에 SIGKILL로 종료됐다.
각 실행의 `.observation.json`과 `.runtime.json`을 별도로 대조했다.

| 후보 | receipt ID | manager 경과 시간 | raw 종료 | 내부 요청 |
| --- | --- | ---: | --- | --- |
| domain transfer | `1795429836851092198-3` | 46.053 s | Unix signal 9 / code 137 | wait 전후 모두 없음 |
| transfer descriptor | `1795430577350827138-3` | 51.463 s | Unix signal 9 / code 137 | wait 전후 모두 없음 |

domain transfer의 compiled source tree는
`5e5f5edc6045dd8d43d02ee6c397579a31de55ad`, 바이너리 SHA-256은
`222eb84120b9172928683dba0d14c4c22d51c319ba1d6da27293c3faa86e42ea`다.
transfer descriptor는 각각 `4708f8d44f59926f7d3acfd5e99d38a66d8b09c1`,
`b3d9020f42fada679f79aafb5fc149a278d3d6a8b9bc276f741f585fa0744dfc`다.

두 실행 모두 receipt의 `termination_audit.attempts=[]`,
`unretained_attempts=0`, stderr의 `termination_attempt_stderr_events=[]`를 확인했다.
`supervisor_termination_reason=null`, timeout 0, pressure events 0,
`automatic_retry=false`, `process_tree_stopped=true`, descendants 0도 동일했다.
따라서 수정 이후의 실제 P7P4 세 실행 모두 기록된 manager 종료 요청 없이 signal 9가
발생했다. 이전의 중복 Drop 재발신을 제거한 소유권 수정은 유지되며, 이 후속 결과도
내부 신호 발신자나 OOM을 확정하는 증거는 아니다.

두 summary에서도 search, identity export, global coverage export, joint comparison의
완료 값은 모두 false였다. 서로 다른 소스·도달 작업량의 42.328/46.053/51.463초를
완료 시간의 개선율로 계산하지 않는다. 원시 자료는 각각
`_local/artifacts/full4-symbolic-domain-transfer-20261123` 및
`_local/artifacts/full4-symbolic-transfer-descriptor-20261123` 접두사의
`.observation.json`, `.runtime.json`이며, 새 탐색·빌드·CI 실행 없이 기존 자료만
문서에 반영했다.

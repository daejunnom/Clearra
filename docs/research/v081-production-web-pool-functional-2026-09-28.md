# v0.8.1 production Web verifier pool 기능 검증 경계

## 범위와 현재 권위

`ebe1bb24c938ac3955975f35bee044130710adbf` 위에 실제 production Web host/pool과
verifier worker를 연결하는 작은 기능 소비자를 추가했다. 기존 realm 검증은 실제
WASM ABI와 독립 메모리를 사용했지만 Web pool의 준비, durable delegation,
consume/finish 및 취소 순서를 직접 실행하지 않았다. 이번 소비자는 그 경계를
검사한다. 아직 실제 WASM을 이용한 새 소비자 실행 결과는 없다.

새 검증 코드는 다음 production 경로를 그대로 사용한다.

- `ClearraVerifierPool`과 `DistributedWasmJobRunner`
- `clearraVerifierWorker.ts` 및 `loadClearraWasmModule`
- `DurableDelegationAuthority`의 실제 offer/start/run/result 전이
- root의 full asset admission, bounded legal synopsis와 relation query/reply ABI

Node worker threads는 격리된 메시지 운송만 제공한다. 가짜 solver/WASM 함수는
없다. `MemoryDelegationJournal`은 명시적인 기능 fixture이며 IndexedDB 내구성
증거가 아니다. 실제 브라우저, OPFS, Tauri IPC, UI 또는 hardware worker/peak
증거로 이 검증을 확대하지 않는다.

## 검사하도록 구현된 요청

다섯 qualified profile의 변경 없는 signed 자료를 재사용한다. 자료 생성·전수
재자격은 하지 않는다. 다음 수치는 기존 기능 증거에서 가져온 검증 예상값이며,
새 Web pool 소비자의 완료 결과가 아니다.

| Profile | 초기 필드 4L / P7 / 6 pieces | empty 4L / IIOOOIIOOO / no-hold |
| --- | ---: | ---: |
| SRS | 245 | 159 |
| SRS+ | 246 | 159 |
| SRS-X | 289 | 159 |
| Jstris 180 | 246 | 159 |
| no-kick | 175 | 159 |

각 입력은 off/off, legal-only, relation-only, combined의 네 정책을 실제
3-worker 분산 실행으로 직렬 baseline과 대조하도록 했다. serial/ready fallback
또는 요청과 다른 plan worker 수는 통과할 수 없다. 전체 canonical identity
sequence, 중복 없음, result hash, coverage 분모/개수와 각 해법 확률·complete
상태를 비교한다. 두 가속기는 적용된 peer admission을 실제 응답으로 확인하고,
relation broker의 질의/응답 및 delegation 메시지 교환을 필수로 요구한다.

전체 pack은 root에만 둔다. peer에 보내는 legal synopsis와 relation seed는 각각
256KiB 이하이며 relation cache 예약은 peer당 1MiB다. 이는 전송 계약 검사이지
전체 실제 peak 메모리 자격을 대신하지 않는다. natural-root topology는 control
root와 별도로 세 compute verifier를 사용하므로 두 자산의 peer 허용 수도 3이다.

추가로 실제 remote consume가 게시된 뒤 취소하고 자산을 끈 새 요청으로
재시작한다. production runner의 취소는 Promise 거절이며, public cancelled
이벤트는 `clearraWorker`가 소유한다. 테스트에서 그 이벤트를 만들어 내거나
runner의 성공 terminal로 오인하지 않는다. 예상 총량은 baseline 10개와 분산
요청 42개(정상 41개, 취소 1개)이며 현재는 **미실행**이다.

## 소스·빌드·실행 정체성

실행기는 정확한 두 fixture root와 현재 source/build contract를 확인한 뒤에만
managed build owner를 연다. WASM source commit, engine identity, artifact 길이와
SHA-256이 일치해야 한다. 허용된 file artifact만 읽으며 외부 HTTP는 없다.
TS 기능 소비자와 worker boot만 명시적인 ESM으로 bundle하며 WASM은 만들지 않는다.
CI의 기존 ordinary WASM 한 번을 재사용한다. profiling/benchmark feature와 포트
4194/4195는 사용하지 않는다.

로컬 일반 WASM 빌드는 감독 영수증
`1790538720679086500-44840-runtime.json`에서 memory-pressure로 실패했다.
return 1, automatic retry false, process tree stopped true, descendants 0이다.
중단 당시 전체 시스템 commit available은 52,518,912바이트였고 owned peak는
4,922,662,912바이트였다. 이를 solver 오류, 성공 build 또는 성능 측정으로
분류하지 않는다. 자원 조건을 바꾼 재시도·WSL 전환은 하지 않았다.

남아 있는 fixture는 `83cb5fdbf2f57fc481d189d72e33a53bc2d328e9`의 manifest다.
현재 `ebe1bb24…` identity로 소비자를 호출하면 managed build owner를 열기 전에
stale WASM을 거절하는 것을 확인했다. 이전 binary를 새 source 검증으로
재명명하거나 fixture identity를 고치지 않았다.

실제 browser 연결은 Node kernel 초기 asset 작성 단계의 OS path-not-found 오류로
초기화되지 않았다. 탭/화면을 검사하지 않았으며 실제 browser 증거는 Open이다.

## 기존 CI 관측과 read-only 실패 수정

[run 36345097318](https://github.com/daejunnom/Clearra/actions/runs/36345097318)은
exact `ebe1bb24…`에서 여섯 job 중 다섯 성공했다. native-products 안의 실제
Desktop native job 검증도 성공했다. 유일한 실패는 read-only 재검증의
`/tmp/Clearra/v081-compute-readonly-36345097318-1`을 Rust 저장 정책이 거절한
것이다. 자료 검증에는 도달하지 않았으며 자산 손상이나 Desktop 실패가 아니다.

새 read-only fixture는 이미 선언된 repository `_local/artifacts` 안의
run/attempt 전용 경로를 먼저 검증한다. 동일 CLI, public production adapter,
서명 자료와 이미 사용한 정확한 Node runtime/library bytes만 준비한다. runner
home/전체 checkout 권한 또는 manager 정책은 넓히지 않는다.

이 bundle만 읽기 전용 bind mount로 공개 Ubuntu 24.04의 resolved image ID에
제공한다. 기존 CLI를 빌드한 job도 Ubuntu 24.04로 고정한다. nobody UID/GID,
read-only root, network none, no-new-privileges, capability 제거와 finite 4GiB/
64-PID 경계 안에서 실제 production `verify`를 실행한다. 기존 container 이름은
거절하고 owned run/attempt container만 종료 trap으로 정리한다. Bash pipefail로
의존 library 복사 중 실패도 유지한다.

이는 읽기 전용 기능 sandbox이며 accepted Bookworm Cloud Run image나 배포
증거가 아니다. image build/push, release receipt 또는 traffic 변경은 없다.
[Docker 실행 제한](https://docs.docker.com/engine/containers/run/)과
[읽기 전용 bind mount](https://docs.docker.com/engine/storage/bind-mounts/)의
별도 계약을 사용한다. 새 Linux 검사 실제 통과는 후속 비게시 CI의 Open 항목이다.

## 이 단계에서 확인한 것과 남은 것

- CI 소스 계약 12개: passed 12, failed 0, skipped 0.
- 새 세 Node 파일 syntax 검사: 통과.
- actual production consumer/worker의 in-memory bundle: 통과, 출력 파일 없음.
- 오래된 WASM의 preflight 거절: 통과, build owner 시작 없음.
- 실제 새 Web pool 실행 및 Linux read-only 실행: 후속 CI에서 확인할 것.
- 실제 browser/OPFS/IndexedDB, strict lint, whole-copy surface parity,
  accepted compute image, aggregate shared peak와 release/readback: 계속 Open.

벤치마크/ABBA, 자산 재생성, v0.9.0 업그레이드, main 병합과 배포는 하지 않았다.

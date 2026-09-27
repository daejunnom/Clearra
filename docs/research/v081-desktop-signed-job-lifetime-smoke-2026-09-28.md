# v0.8.1 실제 Desktop native 자산 수명·결과 검증

## 범위와 권위

후보 `d73c63e3d3fbbe58e78e956267dc4199924d966f` 위에 작은 CLI 소유 통합
target을 추가했다. 실제 `DesktopTauriCommandBridge`, 실제 CLI activation/lifecycle
adapter, 기본 CPU executor와 production system Build admission provider를 사용한다.
가짜 executor, 새 자산 생성, 전수 재자격 또는 benchmark는 사용하지 않았다.

GuiHost → CLI 의존은 dev-dependency를 포함해 저장소 규칙상 금지된다. 따라서
통합 테스트는 `crates/clearra-cli/tests/desktop_signed_accelerator_execution.rs`에
두고 GuiHost만 test-only 의존한다. product dependency/feature는 변경하지 않았다.
workspace dependency graph의 순환 및 GuiHost의 금지 의존 검사는 통과했다.

실제 Tauri command의 parse → active-job admission → native activation → start
순서를 따른다. Tauri 창, async command lock/IPC, 브라우저 host/pool/OPFS 및
accepted container/Cloud Run 검증을 이 증거로 대체하지 않는다.

## 변경 없는 설치 자료와 입력 경계

이전 실제 compute-data smoke가 설치한 열 개 signed slot을 재사용했다.
`_local/artifacts/v081-compute-data-smoke/legal-board`와
`conditioned-reachability`의 정확한 절대 경로를 검사한다. durable Build journal은
관리자가 확인한 `_local/artifacts/v081-desktop-native-smoke/journals`에만 쓴다.
출력 루트의 link escape를 거절한다.

CLI JSON/solution-data 출력 옵션은 CLI presenter 호출에만 붙인다. Desktop에는
실제 GUI의 product-only `clearra-cli/CommandRequest` argv를 전달한다. 검색 인자와
두 가속기 스위치는 같다. `--no-tablebase`는 이를 지원하는 PC에만 명시하며,
Build probability에는 존재하지 않는 TB 옵션을 추가하지 않는다. CLI의 online TB
default feature는 끄지만, GuiHost가 쓰는 기존 WASM 호환 라이브러리는 transitive
App online capability를 포함한다. 이를 제거하거나 TB 업그레이드를 진행하지 않았다.

두 작은 PC 입력은 이전 native compute-data smoke와 같다. 모든 요청은 CPU,
1 worker이며 두 가속기의 off/off, on/off, off/on, on/on을 독립 비교한다.

| Profile | empty 4L / IIOOOIIOOO / no-hold | 초기 필드 4L / P7 / 6 pieces |
| --- | ---: | ---: |
| SRS | 159 | 245 |
| SRS+ | 159 | 246 |
| SRS-X | 159 | 289 |
| Jstris 180 | 159 | 246 |
| no-kick | 159 | 175 |

전체 identity의 순서·중복 없음·hash·coverage 분모/개수·전체 확률·각 해법 확률과
complete 상태를 직접 CLI `run_with_args` 결과와 대조한다. CLI 원문 JSON 숫자를
`RawValue`로 보존해 정확한 float parser로 읽는다. serde_json의 기본 fast-float
재파싱이 일부 확률에 추가한 1 ULP를 제품 차이로 오인하지 않는다. 허용 오차나
production serde feature 변경은 추가하지 않았다.

## 실제 장기 실행 수명

한 bridge를 모든 profile/정책/job에 재사용한다.

- 활성 registry에는 선택한 profile의 켜진 제품만 남아야 한다. 실제 generation과
  profile별 서명 statement identity를 verified embedded authority와 대조한다.
  catalog 파일 전체 identity와 개별 signed statement를 혼동하지 않는다.
- 다섯 profile에서 2L IIOOO minimum의 첫 집합 cardinality 1, known count 1,
  미완 enumeration과 미지 total을 확인한다. bounded `next`로 두 번째 집합을
  생성한 뒤 `get` 재조회가 같은 페이지를 반환하고 첫 집합과 구분됨을 확인한다.
- lazy 페이지를 미리 해제하지 않은 채 다음 요청에서 가속기를 끈다. 이전
  페이지/asset lease가 profile 전환을 가리지 않도록 production 순서를 유지한다.
- 실제 P7 job의 active admission 거절, pre-completion cancel → terminal/drain →
  자산 없는 정상 재시작을 다섯 profile에서 확인한다. Geometry 도중 취소의
  증거로 확대하지 않는다.
- 이미 취소된 열 개 native download는 transport/progress 이전에 취소 오류를
  반환한다. 설치 generation/catalog/payload bytes의 전후 snapshot은 동일하다.
- 작은 실제 Build probability all-solutions 요청도 네 정책에서 CLI와 전체
  결과·확률이 같다. 실제 production system provider를 등록하며 fake admission을
  만들지 않는다.

총 64개 실제 Desktop job(59 completed, 5 cancelled), 11개 CLI baseline과
10개 already-cancelled download를 하나의 bounded functional target에서 검증했다.
테스트 결과는 1 passed, 0 failed, 0 ignored이며 정상 감독 영수증은 다음과 같다.

`1790537538956413600-44300-runtime.json`: return 0, reason normal,
process tree stopped, descendants 0, memory pressure events 0, automatic retry false.
이는 OS peak나 성능·worker scaling benchmark 증거가 아니다.

로컬 compile-time runtime identity는 `d73c63e3…`이며 위의 후속 테스트/manifest
변경을 포함한 로컬 기능 빌드다. 실행 파일 SHA-256은
`142ce80168bbc5fb1301c7229b18625a92a218d0d3ea05e5f6b59e997ce085ba`다.
Cargo 컴파일 완료 뒤 감독의 descendant drain이 실패한 빌드 영수증은 성공으로
바꾸지 않았다(`1790537468102935300-10680-runtime.json`, return 125,
tree-not-stopped, 최종 owned-tree cleanup). 자원 조건을 바꾼 OOM 재시도는 없었다.
독립 정상 기능 실행과 실패한 build authority를 분리한다.

## 비게시 CI 수정과 남은 경계

이전 exact source `d73c63e3…`의 run 36341358234는 여섯 job 중 다섯 성공,
native-products의 비권한 read-only step만 실패했다. Node가 checkout 내 실제
스크립트를 찾지 못했고 자산 검사에는 도달하지 않았다. 이 실패를 자산 손상으로
분류하지 않는다.

새 workflow는 설치가 성공한 동일 열 개 slot으로 Desktop target을 실행한다.
read-only 검증은 runner home/전체 checkout의 권한을 넓히지 않는다. 관리자가
검사한 run/attempt 전용 `/tmp/Clearra` 경로에 공개 스크립트·동일 CLI·서명 데이터만
복사하고 실행 파일/스크립트 bytes를 대조한다. 전용 bundle의 write를 제거하고
nobody의 script read, CLI execute, data non-writable을 선행 확인한 뒤 production
`verify`를 실행한다. 이 Linux 수정의 실제 통과 여부는 새 CI로 확인해야 한다.

CI source 계약 11개, 새 Rust test formatting과 diff 검사는 통과했다. main 병합,
배포, 4194 교체, 벤치마크, v0.9.0 업그레이드는 하지 않았다.

여전히 Open: 실제 Tauri/GUI 및 browser/OPFS, 전체 copy/export surface parity,
Linux read-only 재검증, 성공 build supervision, strict lint, shared peak/성능,
최종 exact-SHA acceptance와 배포/readback. 이번 기능 증거는 기존 자산의
Qualified 상태를 확대하거나 release를 완료 처리하지 않는다.

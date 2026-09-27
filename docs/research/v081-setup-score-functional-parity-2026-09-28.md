# v0.8.1 Setup-score 실제 실행 동등성 보강

## 범위

`codex/v081-selective-source-ci-20260927`의 후속 기능 검증이다. 기존 PC/Build
reducer 검증에서 빠져 있던 `setup score`의 실제 Build coverage, PC score
continuation, 공통 ranked reduction 및 Discord 전달 경계를 추가한다. solver,
점수 의미, canonical-only 정책, 닫힌 Build registry, 기존 signed generation은
바꾸지 않는다. 자료 재생성·재자격, 벤치마크, v0.9.0, main 병합과 배포는 제외한다.

## 먼저 확인한 CI 결함

[run 36350733063](https://github.com/daejunnom/Clearra/actions/runs/36350733063)은
exact `6f01d4460be8f229b8c91ec1606b6fbdca1022d5`에서 6개 작업 중 5개 성공,
`native-products` 실패로 종료했다. 실패 step은 새 실제 CLI→Discord 소비자이며,
`pc score-minimals` fixture가 명시 `--backend cpu`를 넣었기 때문이다.
제품 parser는 CPU/no-fallback을 내부에서 고정하고 그 override를 의도적으로
거절한다. `E_CLI_INVALID_VALUE`를 없애려고 제품 제한을 풀지 않고 fixture의
불필요한 옵션만 제거했다. 다른 PC 제품의 명시 CPU 옵션은 유지한다.

이 실패 뒤의 독립 Desktop 실제 실행과 network-none/read-only 비특권 검사는
통과했다. 같은 실행의 `product-wire-ui`도 네 정책 각각 렌더 100개, 전체 복사
246개, member pages 3개로 통과했다. 기존 실패는 실패로 보존하며 이후 수정의
검증 권위로 재명명하지 않는다.

## 추가된 실제 입력과 검사

CTK3 `ctk3_w0kGEPVAACzgA2A9EAAw3A`는 세 source page를 담는다. 빈 필드의
가로 I가 bottom row의 왼쪽 네 칸 또는 오른쪽 네 칸을 차지하고, 첫 페이지가
한 번 반복된다. 저장소 CTK3 codec으로 실제 점유 `0xf, 0x3c0, 0xf`를 확인한다.
Setup queue는 `I`, continuation queue는 `OOOI`, target은 2L, hold 없음,
score profile은 TETR.IO다. 만든 solver 결과를 입력으로 주입하지 않는다.

- 실제 source page 3개가 candidate 2개로 중복 제거된다.
- 두 candidate의 Setup coverage와 continuation PC 확률이 모두 1이다.
- 두 점수가 같고 양수이며, 평균과 각 candidate 점수가 같다.
- 동률 순서는 작은 canonical candidate identity 우선이다.
- 네 accelerator on/off 조합에서 전체 public Setup payload가 같다.
- App에서는 요청 worker 1·2·11 각각 동일 결과를 대조한다. 실제 사용 worker나
  병렬 성능의 증명으로 확대하지 않는다.
- Discord 소비자에는 다섯 profile × 네 정책의 Setup 요청 20개를 더해 총
  85개 실제 요청을 준비한다. 기존 ordinary CLI와 설치한 10개 signed slot,
  production runner/direct executor를 그대로 재사용한다.

처음 App 실행에서는 테스트가 native Build admission host를 등록하지 않아
`native_build_probability_host_provider_not_registered`로 실패했다. production
`SystemNativeBuildProbabilityAdmissionProvider`를 기존 managed functional root의
전용 journal namespace에 한 번 등록하도록 보완했다. fake provider, source SHA
placeholder, admission 우회 또는 임시 전역 자원 제한 변경은 사용하지 않는다.

## 증거와 남은 경계

현재 source/입력 계약 16개와 read-only parser 3개가 로컬에서 통과했다. 이전
ordinary CLI의 `d73c63e3d3fbbe58e78e956267dc4199924d966f` binary로 잘못된 score
옵션의 거절, 제거 후 PC 세 제품/Build projection 동등성, Setup의 실제 두 결과와
양수 동률 점수를 확인했다. 이는 원인 진단용 이전 binary 증거이며 현재 exact
source의 통과로 기록하지 않는다.

host 등록 수정 뒤 새 App target은 컴파일을 완료했지만 Windows application control
정책의 `os error 4551`로 실행 파일이 시작되지 않았다. 같은 자원·동일 binary의
수동 재시도 한 번도 같은 단계에서 차단됐다. 테스트 성공으로 기록하지 않으며,
WSL 우회, 정책 변경, 자원 확대 또는 자동 반복은 하지 않았다. 첫 실패와 재컴파일
감독은 compiler descendant drain 경고도 남겼으나 receipt의 최종
`process_tree_stopped`는 true다. 이를 깨끗한 supervision 성공으로 기록하지 않는다.
변경 전부터 있는 no-render App의 dead-code 경고 두 개도 strict lint와 구분한다.

새 App actual execution 및 이미 보유한 SRS+ signed pair 검증은 후속 CI에서
같은 test target을 재사용한다. 새 85개 CLI→Discord 실제 실행도 후속 exact-source
CI의 Open 항목이다.
accepted Bookworm compute image, Cloud Run/Gateway/attachment, 실제 browser의
OPFS/IndexedDB/clipboard/cross-tab lease, Tauri IPC, 전체 Setup domain, strict
lint, shared peak, 성능 gate와 release도 별도 Open이다.

같은 `6f01d446` Core CI에서 adaptive memo의 작은 요청 flat 유지, 큰 memo의
무손실 State-major 전환, 전환 실패/범위 밖 보존, 요청·epoch 격리 테스트가
통과했다. 기존 제품 adaptive 기본값은 유지하며 속도나 OS peak 이득으로
확대하지 않는다.

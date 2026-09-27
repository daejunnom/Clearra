# v0.8.1 실제 CLI 결과의 production Discord 실행 경계

## 범위와 권위

기준 소스는 `codex/v081-selective-source-ci-20260927`의
`26630d246e54642875c3486a5d95bccb1cd53e3d`다. 이전 기능 검증은 실제 CLI와
서명 자료 adapter, Desktop native job 및 production Web verifier pool을
대조했다. Discord 결과 projection의 기존 작은 단위 테스트는 만들어진 JSON을
사용했으므로 실제 CLI stdout이 production runner와 direct executor를 연속
통과하는 기능 소비자를 별도로 연결한다.

production solver와 Discord의 canonical-only 선택, 닫힌 Build 옵션 registry는
변경하지 않는다. Cloud Run 실행·Gateway 메시지·첨부 이미지/GIF·성능 동등성
검증은 아니다. 벤치마크, 자료 재생성·재자격, v0.9.0, main 병합과 배포는 제외한다.

## 실제 실행 소비자

`realCliProductProjection.test.mjs`는 명시한 ordinary CLI와 이미 설치한 열 개
서명 slot만 사용한다. 같은 자료를 production adapter의 `verify`로 검사하며
재다운로드하거나 generation을 교체하지 않는다. 지정 binary는 repository의
기존 Cargo debug/release 경로, 자료는 기존 declared smoke root로 제한한다.
일부 환경값만 지정하거나 compiled runtime identity가 exact source SHA와 다르면
실패한다. 설정이 전혀 없는 일반 test 발견에서만 skip한다.

작은 요청은 다음과 같이 총 65개다.

| 실제 요청 | 입력 | profile/정책 범위 |
| --- | --- | --- |
| `pc minimals` | 빈 2L, `IIOOO`, hold 없음 | 다섯 profile × 두 가속기 네 조합 |
| `pc score-minimals` | 같은 빈 2L/fixed queue | 다섯 profile × 네 조합 |
| `pc path` | 초기 mask `0x3f0`, 1L, `I`, hold 없음 | 다섯 profile × 네 조합 |
| `build cover` | base 0, target 15, height 4, `I`, 명시 oracle/min-cover/CPU | 다섯 profile, 기존 기본 가속 정책 |

PC의 accelerator/TB 명시 옵션은 현재 registry 경로를 사용한다. Build의 닫힌
registry에는 그 override 권위가 없으므로 옵션을 추가하거나 네 정책을 지원한다고
기록하지 않는다. 같은 실제 요청 목록을 작은 source 계약 테스트에서도 production
`prepareClearraArguments`에 통과시켜 필수 queue-knowledge/objective/fallback
정책을 빠뜨린 fixture를 실제 CLI 컴파일 전에 거절한다.

각 요청은 production argv 정규화를 거친 CLI baseline을 한 번 실행하고, 같은
입력을 `ClearraDirectExecutor`의 기본 `ClearraCommandRunner`로 다시 실행한다.
fake runner/spawn이나 JSON을 입력으로 주입하지 않는다. compiled identity와 실제
typed 결과 kind, 정상 종료, CLI의 canonical projection과 runner/direct 결과의
동등성, 세 번째 projection의 멱등성을 대조한다. 실행 전후 열 slot의 catalog,
generation과 설치 bytes가 유지되는지도 검사한다. 이 단계는 1-worker 기능
경계이며 worker 성능이나 전체 candidate universe의 전수 증명으로 확대하지 않는다.

## CI와 현재 상태

기존 `native-products` job의 ordinary CLI와 signed data provision 뒤에 소비자를
붙인다. Cargo/pnpm/WASM 빌드나 data download를 추가하지 않고 동일 artifact를
읽는다. 다른 독립 검사는 실패한 소비자 때문에 생략하지 않되 원래 실패는 유지한다.

- `Implemented`: 실제 CLI→runner→direct→canonical 소비자와 source-bound CI step.
- `Locally validated`: source 계약 15개와 read-only parser 3개, 총 18 passed,
  0 failed, 0 skipped. 65개 fixture의 기존 registry admission도 여기 포함된다.
- `Open`: 이 새 소비자의 65개 실제 CLI/Discord 실행. 후속 exact-source
  비게시 CI에서 확인한다. source 계약 통과를 실제 실행 통과로 기록하지 않는다.
- `Open`: 실제 accepted compute image/Cloud Run, Gateway/attachment readback,
  실제 browser/Tauri IPC, shared peak, 성능, 최종 exact-SHA acceptance와 release.

이전 CI의 UI 실패와 실제 246-member copy 재검증은
[다중 member 기록](v081-multi-member-copy-source-bound-2026-09-28.md)에 별도로 남긴다.

## 후속 실제 CI와 Setup 확장

exact `6f01d446`의 [run 36350733063](https://github.com/daejunnom/Clearra/actions/runs/36350733063)에서
새 실제 소비자는 `pc score-minimals` fixture의 명시 `--backend cpu` 거절로
실패했다. CPU-only/no-fallback 제품 계약은 유지하고 fixture override만 없앤다.
다른 독립 검사는 계속되어 Desktop 및 read-only 단계가 통과했고, 여섯 job 중
다섯 job은 성공했다. 실제 소비자의 실패를 source 계약 성공으로 덮지 않는다.

후속 소비자는 실제 Setup-score coverage/continuation/ranking을 네 정책·다섯
profile에서 확인하는 20개 요청을 추가해 총 85개를 실행한다. CTK3 실제 입력
decode, 요청 admission 및 제품-owned 실행 옵션 부재도 source 계약에 포함한다.
[Setup 실제 실행 기록](v081-setup-score-functional-parity-2026-09-28.md)의 로컬
증거와 후속 exact-source CI 증거를 구분하며 accepted image/Gateway 또는
전체 candidate universe 완료를 주장하지 않는다.

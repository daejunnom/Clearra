# Clearra 저장공간 누적: 실측 및 소스 감사

측정: 2026-09-13 01:00~01:25 KST. 소스 기준:
`codex/v0.9.0-stacked-on-v0.8.1-20260912`, `bbc3e2d` 및 동일한 보존 정책을 가진 작업 트리.
이 문서는 삭제 계획을 실행한 결과나 제품 릴리스 완료 증거가 아니다.

## 결론

Temp 이외의 대용량은 주로 **분산된 Cargo 빌드 산출물과 실험 작업 트리**다.
Windows `%LOCALAPPDATA%/Clearra`만 약 **122.89 GiB**, 원래 저장소의
`target`은 별도로 **12.88 GiB**였다. WSL 안에서도 Clearra 캐시가
**68.58 GiB**였다. 제품의 검색 결과나 TB 데이터가 100 GB 다운로드됐다는
증거는 발견하지 않았으며, 이번 조사로 확인한 대용량 경로는 개발·검증 산출물이다.

기존 정책은 `build` 디렉터리 하나의 용량, 한 Cargo target 안의 디버그 세대,
한 WASM 게시 위치의 세대를 각각 관리한다. 이들을 모두 합한 전체 Clearra
사용량이나 실험 target 디렉터리 개수에 대한 상한은 없다. 병렬 구현 중
각 작업 트리에서 독립 target을 만든 실행도 이 누적에 포함된다.

## 실측 범위와 중복 합산 주의

비밀 파일의 내용은 열지 않았다. 생성물도 내용 대신 `du -k -d N`, 파일
크기 속성, `df`로 용량만 확인했다. 단위는 GiB = 1024³ bytes다.
아래 상위 디렉터리와 하위 디렉터리는 **합산하지 않는다**.
실시간 사용 중인 경로라 측정 시점에 따라 소량의 차이가 날 수 있다.

### Windows

`LC`는 `C:/Users/강민수/AppData/Local/Clearra`, `REPO`는
`C:/Users/강민수/Desktop/프로젝트/Clearra/Clearra`다.

| 위치 | 측정 KiB | GiB | 관계/분류 |
| --- | ---: | ---: | --- |
| `LC` 전체 | 128,864,343 | 122.89 | 아래 worktrees/codex/build 등을 포함 |
| `LC/worktrees` | 92,605,389 | 88.32 | 실험·구현 소스와 각자의 빌드 산출물 |
| `LC/codex` | 27,574,847 | 26.30 | 과거 감사·검증용 독립 target |
| `LC/benchmarks` | 4,664,297 | 4.45 | 벤치마크 관련 파일 |
| `LC/baseline-audit` | 1,909,548 | 1.82 | 기준 버전 감사 파일 |
| `LC/build` | 1,176,265 | 1.12 | 중앙 8 GiB 정책이 직접 관리하는 루트 |
| `REPO/target` | 13,505,078 | 12.88 | 위 LC와 별개; 사실상 전부 debug |
| `REPO/../.codex-target-contract-final` | 4,500,413 | 4.29 | 위 LC와 별개인 예전 독립 target |

대표적인 큰 작업 트리:

| `LC/worktrees/` 아래 | 전체 GiB | 직접 확인한 내부 빌드 용량 |
| --- | ---: | --- |
| `pc-all-worker-scheduling-20260912` | 22.31 | `target` 23,289,371 KiB = 22.21 GiB |
| `v081-minimum-lazy-contract-20260912` | 9.95 | 작업 트리 전체 측정; 삭제 분류 전 추가 확인 필요 |
| `deploy-v2-minimum-20260912` | 9.45 | `target` 9,704,301 KiB = 9.25 GiB |
| `v090-input-policy-boundary-20260912` | 7.97 | 작업 트리 전체 측정 |
| `v081-product-gap-audit-20260912` | 7.61 | 작업 트리 전체 측정 |
| `v090-pc4-observation-outcome-ledger-20260912` | 5.66 | 진행 중인 구현 소스 포함; 작업 트리 자체는 삭제 대상 아님 |
| `v090-partial-profile-activation-20260912` | 4.43 | 작업 트리 전체 측정 |

`LC/codex/finesse-target`만 9.64 GiB,
`finesse-audit-target` 4.57 GiB,
`release-v072-acceptance-target` 4.08 GiB다.

별도로 npm cache 5.67 GiB, Playwright 브라우저 배포본 1.35 GiB,
Codex 대화 기록 4.54 GiB, Windows Cargo registry 1.01 GiB,
Rust toolchains 2.48 GiB도 확인했다. 이는 다른 프로젝트나 사용자 작업과 공유될
수 있으며, 특히 대화 기록은 재생성 가능한 빌드 캐시로 취급하지 않는다.

### WSL Ubuntu

| 위치 | 측정 KiB | GiB |
| --- | ---: | ---: |
| `/home/stemxstudio/.cache/Clearra` | 71,912,536 | 68.58 |
| 그 안의 `build` | 69,340,896 | 66.13 |
| 그 안의 `build/cargo-target` | 43,845,132 | 41.81 |
| 그 안의 `build/cargo-target-wasm` | 8,117,080 | 7.74 |
| `/home/runner/.cache/Clearra` | 244,508 | 0.23 |
| `/home/stemxstudio/.local/share/Clearra` | 299,956 | 0.29 |

기본 `build/cargo-target` 내부에서는 `debug/incremental` 20.63 GiB,
`debug/deps` 18.20 GiB, debug 전체 39.26 GiB였다. WASM용 target도
호스트 검증을 위한 debug 파일이 함께 있고 `debug/incremental`만 3.41 GiB다.

Windows의 Ubuntu `ext4.vhdx` 파일 길이는 136,762,621,952 bytes
(127.37 GiB), Linux `df -h /`의 실제 파일시스템 사용 표시는 121 GiB였다.
위 Linux 캐시는 이 가상 디스크 **안에 포함**되므로 VHD와 합산하지 않는다.
VHD 전체가 Clearra 파일인 것도 아니며, 파일 길이가 즉시 회수 가능한 공간을
뜻하지도 않는다. VHD 삭제·압축·WSL 종료는 수행하지 않았다.

## 소스에서 확인한 누적 경로

### 1. 8 GiB 상한의 범위는 build 한 곳

- `scripts/lib/clearra-path-helpers.ps1:16`: Windows 기본 root는
  `%LOCALAPPDATA%/Clearra/build`; Linux는 XDG cache 아래 `Clearra/build`다.
- `scripts/lib/clearra-artifact-cache.ps1:3,296`: 기본 상한은 8 GiB이고,
  retention은 전달받은 artifact root **하나**만 측정한다.
- 같은 파일 `314`: 초과 시 오래된 파일만 지우는 정책이 아니라 그 artifact
  root를 reset한다. 감사용으로 이 함수를 호출하면 안 된다.
- `worktrees`, `codex`, 임의 외부 target, 과거 benchmark 경로는 상한 밖이다.
- `CLEARRA_MAX_BUILD_CACHE_GIB`는 현재 Windows Process/User/Machine 및
  이번 WSL shell에 설정돼 있지 않았다. 과거 실행의 환경은 확정하지 않는다.

### 2. 최신 1개는 모든 임시 빌드 중 하나를 뜻하지 않음

- `scripts/tools/retain-clearra-debug-builds.ps1:54`: 선택한 target의 바로
  아래 `debug`만 다룬다. 다른 target, release, target-triple 하위는 제외다.
- 같은 파일 `93`: incremental은 compile unit별 최근 variant 하나다.
- 같은 파일 `116`: 실행 파일은 package/target-kind별 최근 exe와 연관
  pdb/d/exp를 남긴다. Linux ELF의 일반 세대 회수 정책이 아니다.
- 공유 rlib/rmeta, build-script dependency, fingerprint는 의도적으로 남긴다.
  다른 결과의 링크에 필요하므로 파일 몇 개만 임의 삭제해서는 안 된다.
- 같은 파일 `63`: Windows에서 cargo/rustc/link가 실행 중이면 skip한다.
  활성 lock, 소유권 불명, reparse 등도 정리를 막을 수 있다.

### 3. 정리 hook를 우회하는 독립 실행

- `.cargo/config.toml`: runner가 `CARGO_TARGET_DIR`을 지정한다는 주석만
  있으며 bare Cargo를 강제로 중앙 root로 보내는 설정은 없다.
- `scripts/tools/invoke-clearra-debug-cargo.ps1:24`: incremental을 끄고
  선택한 target의 debug 세대를 정리하지만 중앙 8 GiB budget은 호출하지 않는다.
- `scripts/clearra.ps1:108,132,227`: 정상 try/finally에는 회수가 있지만,
  try 진입 전 인자/실행 표면 검증 실패나 프로세스 강제 종료는 다르다.
- `scripts/lib/progress/native_progress_runner.ps1:151,166`: debug 회수가
  process 정리 finally 뒤에 있어 시작/출력 처리 예외는 해당 회수를 건너뛴다.
  정상 Cargo nonzero 종료가 항상 정리를 건너뛰는 것은 아니다.
- `scripts/verify.ps1:267`: 단독 실행의 finally에는 중앙 종료 budget hook이
  없고 최상위 runner에 기대는 구조다.

### 4. WASM 게시 세대와 컴파일 캐시는 별개

- `scripts/tools/build-clearra-wasm.mjs:110,145`: WSL/native의 별도 target을
  사용하며 override도 허용한다. standalone JS builder는 중앙 시작/종료
  retention을 호출하지 않는다. 따라서 기본 build 하위여도 독립 실행만으로
  상한이 항상 적용된다고 볼 수 없다.
- `scripts/tools/clearra-wasm-generation-retention.mjs:54`: 5세대 정책은
  destination의 JS/WASM 쌍에만 적용된다. Cargo 캐시를 5개로 제한하지 않는다.
- 미완성/orphan/history 불일치 시 게시 세대 삭제는 안전상 skip한다.

### 5. 실험·작업 트리·WSL source 복사본의 수명주기 부재

- `scripts/lib/clearra-runtime-environment.ps1:175,232`: Windows source
  절대경로의 hash마다 WSL workspace 복사본을 만들고 현재 ID 내부만 교체한다.
  다른 ID의 GC는 이 함수에 없다. 이번 기기에서 이 복사본은 0.26 GiB여서
  주범은 아니며, 가능한 누적 경로와 실제 대용량을 구분한다.
- 추적된 scripts에는 Windows worktrees 전체 수명관리 구현이 발견되지 않았다.
  작업 트리 소스 보존과 그 안의 generated target 회수는 별개의 작업이다.
- `scripts/benchmark/run-finesse-benchmark.mjs:21` 및
  `scripts/benchmark/run-wasm-p7-matrix.mjs:11`: 임의 report 경로를 허용하며
  자체 전체 상한은 없다. 중앙 reports의 14일/200개/256 MiB 정책은 다른
  report 디렉터리까지 포괄하지 않는다.

## 안전한 후속 처리 경계

이번 요청은 원인 조사이므로 **캐시/작업 트리/대화 기록을 삭제하지 않았다**.
4194 서버와 게시된 WASM도 변경하지 않았다. 현재 C: 여유 약 11.85 GiB로
새 대형 Rust/WASM 빌드를 시작하지 않고 소스 통합·빌드 없는 검사를 우선한다.

공간 회수 시에는 먼저 활동 중인 compiler/test/서버, Git 변경, 미병합 커밋,
릴리스 증거와 생성물 소유권을 대조해야 한다. 다음 순서가 적절하다.

1. 사용 종료한 **명시된 target의 generated 파일만** 회수한다. worktree 자체,
   source, 사용자 입력, 배포 산출물·기록은 보존한다.
2. 이후 실험은 용도별 고정 target slot을 재사용하고 incremental/debug-info
   정책을 runner에서 통일한다. 서로 다른 ABI/toolchain/profile은 분리한다.
3. standalone WASM/debug/verify도 공통 owner/lease와 budget accounting에
   연결한다. Windows와 WSL 각각의 budget, busy/skip/last-success 상태를 보인다.
4. 임의 외부 target은 소유권 registry에 등록하거나 unmanaged로 명시한다.
   활성 owner의 빌드를 삭제하는 방식으로 상한을 강제해서는 안 된다.
5. WASM 5세대, debug unit 1세대, 전체 byte budget, worktree 수명주기를
   별도 정책으로 유지하고 총합을 보여준다. 단순히 숫자를 1로 바꿔서는 안 된다.

이 개선안은 감사 결과이며 아직 retention 코드에 적용하지 않았다.

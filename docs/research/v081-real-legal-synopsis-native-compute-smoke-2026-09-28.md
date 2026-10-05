# v0.8.1 실제 legal-board synopsis·native compute-data 기능 검증

## 기준과 권위

후보는 `codex/v081-selective-source-ci-20260927`, 이번 수정의 기준 소스는
`12f8587ec0b9b5e16e13aaa364e36045c6f30675`다. 이 exact SHA의 비게시 CI
[`36339168944`](https://github.com/daejunnom/Clearra/actions/runs/36339168944)는
Core, native products, WASM ABI, 실제 WASM realms, surfaces, product-wire-ui
여섯 작업 모두 성공했다. 아래 추가 검증을 그 이전 CI 결과로 대신하지 않는다.

이번 작업은 이미 자격을 얻은 다섯 profile의 두 자산 제품을 실제 제품 ABI와
CLI에 연결하는 작은 기능 검증이다. 자산을 재생성하거나 전수 재자격하지 않는다.
벤치마크, ABBA, P7P4, 프로파일링, 시간·peak 비교, TB 업그레이드, main 병합,
배포 및 기존 4194 세션 교체는 범위 밖이다. solver/reducer 의미는 변경하지 않았다.

## 실제 WASM legal-board owner·synopsis peer

`apps/clearra-web/test/realAcceleratorRealms.test.mjs`의 두 번째 테스트는 기존
ordinary WASM을 다시 빌드하지 않고 compile-source 계약, 파일 길이·SHA-256 및
실제 응답 runtime identity를 확인한다. Rust/WASM compile identity는 변경 없는
`83cb5fdbf2f57fc481d189d72e33a53bc2d328e9`이며 이전 컴파일 영수증의 cache seed
봉인 실패는 [이전 기록](v081-real-wasm-owner-peer-smoke-2026-09-28.md)에 보존한다.
이 파일을 릴리스 산출물 또는 성공 build receipt로 승격하지 않는다.

full pack은 owner 하나에만 admission한다. 두 실제 Node worker는 각각 독립
WASM memory/registry를 가지며 256KiB 이하의 음성 전용 synopsis만 받는다.
각 profile의 embedded signed catalog, full payload 길이·digest, synopsis 길이를
확인하고 실제 `export_negative_synopsis`/`admit_negative_synopsis` ABI를 호출한다.

각 peer는 다른 profile에 대한 admission과 손상된 synopsis를 각각 거절한다.
손상 admission 뒤에도 이전 유효 synopsis의 결과가 보존된다. scope 안의 empty
4L와 scope 밖의 2L, 초기 필드 1L, 초기 필드 4L에서 full-owner/peer와 no-asset
baseline의 전체 identity 순서·집합 hash·coverage denominator/count·확률·해법별
확률을 대조한다. 제거 뒤에는 스위치를 켠 채 exact fallback이 같은 결과를
반환하는지 확인한다. SRS+의 두 peer에는 cancel→drain과 정상 재실행도 포함한다.

입력은 모두 공통 `clearra pc` ingress, `--objective unique --count unique
--solution-probabilities --backend cpu --workers 1 --no-tablebase
--no-conditioned-reachability --rule PROFILE`이며 legal-board만 독립 on/off한다.

| 입력 | 나머지 명시 옵션 |
| --- | --- |
| empty 4L, eligible | `--lines 4 --height 4 --board-mask 0 --pieces 10 --queue IIOOOIIOOO --no-hold` |
| empty 2L, out-of-scope | `--lines 2 --height 2 --board-mask 0 --pieces 5 --queue IIOOO --no-hold` |
| 초기 필드 1L, out-of-scope | `--lines 1 --height 1 --board-mask 0x3f --pieces 1 --queue I --no-hold` |
| 초기 필드 4L, out-of-scope | `--lines 4 --height 4 --board-mask 0x3c0f03c0f --pieces 6 --patterns P7 --hold empty` |

| Profile | synopsis bytes | eligible empty 4L | outside 2L | outside initial 1L | outside initial 4L |
| --- | ---: | ---: | ---: | ---: | ---: |
| SRS+ | 246,341 | 159 | 4 | 1 | 246 |
| SRS | 246,341 | 159 | 4 | 1 | 245 |
| SRS-X | 246,469 | 159 | 4 | 1 | 289 |
| Jstris 180 | 246,341 | 159 | 4 | 1 | 246 |
| no-kick | 229,957 | 159 | 4 | 1 | 175 |

새 자료 자격 검사가 아니라, 기존 qualified 자료를 이용한 기능 검증이다.
97개 정상 탐색, 2개 취소, 20개 기대된 admission 거절을 포함한다. 기존 relation
테스트까지 함께 실행한 결과는 2개 통과, skip 0이다. 정상 감독 영수증은
`1790532795653650000-38324-runtime.json`, return code 0, reason `normal`,
owned tree stopped, descendant 0, memory-pressure event 0이다.

각 realm은 1-worker solver 요청이다. 이를 11-worker 탐색·성능, prune/hit rate,
128MiB 공유 자산 peak 또는 실제 browser pool/OPFS 검증으로 해석하지 않는다.
synopsis는 음성 필터만 제공하며 positive 값에 도달성 권위를 주지 않는다.

## 실제 CLI와 production compute-data adapter

`apps/clearra-discord-bot/test/realComputeAccelerators.test.mjs`는 production
`prepareComputeAccelerators`의 기본 실제 CLI 호출을 사용한다. fake invoke,
대체 solver, 가짜 activation journal이나 qualification receipt는 만들지 않는다.
자산 저장 루트는 `_local/artifacts/v081-compute-data-smoke` 하나로 검증하며
ordinary CLI의 절대 경로와 40자리 compile source identity를 명시한다.

네트워크 provision은 `CLEARRA_REAL_COMPUTE_MODE=provision`의 명시 경로만 허용한다.
기본은 `verify`다. 자산/CLI/source 세 입력이 모두 없을 때만 일반 테스트에서
skip하고, 일부만 제공한 잘못된 설정은 실패한다. 기존 두 signed Release의
다섯 profile을 실제 CLI download로 설치하며 사용자 전역 저장소는 변경하지 않는다.

production adapter는 첫 다운로드 전에 열 개 signed catalog slot과 크기 상한을
검사하고 설치 세대·바이트 수·catalog identity를 대조한다. 설치 후 실제 `verify`
경로와 별도 status snapshot을 통해 같은 열 개 generation을 확인한다. 이어서
위 eligible empty 4L 및 outside initial 4L 입력을 다섯 profile에서 각각 네 정책
조합(off/off, legal만, relation만, 둘 다)으로 실행한다.

40개 실제 CLI 요청의 complete solution identity 순서·집합 hash·coverage·해법별
확률이 같은 profile의 baseline과 일치했다. 선택 relation의 policy와 actual
snapshot 활성 상태도 대조하며 초기 필드에서는 legal-board negative prune이
0인지 확인한다. SRS+ eligible 입력의 legal-only 재조회는 159개 complete 결과와
108개 verified negative prune을 보였다. 하네스는 이 경로의 prune이 양수인지도
요구하여 admission만 하고 solver에서는 사용하지 않는 회귀를 거절한다. 이 작은
기능 관측을 성능 이득으로 해석하지 않는다. PC4 TB 및 backend fallback은 명시 off다. 탐색 전후 설치된
catalog/generation/bytes snapshot이 동일하다. 위 WASM 표와 두 입력의 결과
개수도 같다. 다른 profile의 결과를 동일한 것으로 취급하지 않는다.

검증 binary는 `build/cargo/default/debug/clearra.exe`의 ordinary build이며
`--no-default-features --features wasm-cpu-runtime`만 사용했다. 실제 응답의
`source_commit`과 `engine_build_id`는 모두 기준 소스 `12f8587e…`다. SHA-256은
`b2bd164c504da55d06de2c5cc359992dbca9c0236a6fe7f66c613f320f72e75b`다.

Cargo 컴파일은 완료됐지만 감독 빌드 영수증
`1790533008303858700-41540-runtime.json`은 종료 시 descendant 1개가 남아
`E_CLEARRA_PROCESS_TREE_NOT_STOPPED`, return code 125, reason `tree-not-stopped`다.
감독의 강제 정리 후 `process_tree_stopped=true`이며 컴파일러는 남아 있지 않았다.
메모리 pressure event는 3회다. 이 빌드 기록을 성공·acceptance로 바꾸지 않고,
재빌드 또는 자원 변경 재시도도 하지 않았다. 독립 identity를 확인한 바이너리로
기능 검증만 진행했다.

실제 native adapter 테스트는 1개 통과, skip 0이다. 정상 영수증은
`1790533589453833500-38940-runtime.json`, return code 0, reason `normal`,
owned tree stopped, descendant 0, memory-pressure event 0, automatic retry false다.
이 검증은 native provisioning/consumption 경계이며 실제 read-only Linux container,
권한 하강한 accepted image, job-service HTTP 및 Cloud Run의 증거는 아니다.

## 비게시 CI와 아직 남은 경계

새 CI는 `wasm-realms`에 변경 없는 legal-board 다섯 파일을 추가한다. ordinary
WASM producer는 여전히 한 번이며 peer에는 작은 synopsis만 전달한다.
`native-products`는 exact CI SHA를 identity로 쓰는 ordinary CLI 하나를 만들고
위 실제 adapter 테스트를 실행한다. 이후 같은 자산 디렉터리만 쓰기 금지로
바꾸어 `nobody` 사용자에게 실제 `verify`를 실행시킨다. 이 마지막 Linux 권한
검사는 준비된 CI 경로이며 로컬 Windows에서 실행됐다고 기록하지 않는다.
실패는 그대로 유지하고 다른 독립 source feedback을 차단하지 않는다.

CI source 계약 10개와 기존 image mock 계약 4개는 로컬 통과했다. 새 exact SHA의
CI 관측은 별도다. 새 asset qualification, acceptance, image build, Cloud API,
Release, promote나 배포 권위를 만들지 않는다.

여전히 남은 항목:

- 실제 browser host/pool/broker/OPFS의 자산 설치·readback·세션 교체
- Desktop의 장기 실행 native owner와 실제 설치·취소·profile 교체 smoke
- accepted Discord compute 이미지의 실제 권한 하강·read-only 자산 소비 smoke
- strict lint, active-session 실제 공유 peak 및 이후 성능 gate
- exact source acceptance, 배포 및 rollback readback

현재 자료 자격과 새 기능 증거를 구분한다. v0.8.1 전체 완료 또는 Released로
표시하지 않으며, 벤치마크와 v0.9.0 업그레이드는 다음 단계로 유지한다.

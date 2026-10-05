# v0.8.1 실제 WASM owner·bounded peer 기능 검증

## 범위와 기준 소스

후보는 `codex/v081-selective-source-ci-20260927`이며 Rust/WASM 기준 소스는
`83cb5fdbf2f57fc481d189d72e33a53bc2d328e9`다. 해당 exact SHA의 비게시 CI
[`36336975081`](https://github.com/daejunnom/Clearra/actions/runs/36336975081)는
Core·native products·WASM ABI·surfaces·product-wire-ui 다섯 작업 모두 성공했다.
아래 새 실제 WASM realm 검증의 성공을 그 이전 CI로 대신하지 않는다.

이번 단계는 작은 기존 6미노 P7 입력과 변경 없는 다섯 서명 relation pack을
실제 제품 WASM scalar ABI에 연결하는 기능 검증이다. 벤치마크·ABBA·P7P4,
프로파일링, 시간·peak 비교, 자료 재생성·전수 재자격, TB 업그레이드,
main 병합·배포·4194 교체는 범위 밖이다. solver나 reducer 의미를 변경하지 않는다.

## 실제 realm과 공개 ABI 경계

`apps/clearra-web/test/realAcceleratorRealms.test.mjs`는 다음을 실행한다.

1. 일반 `build-clearra-wasm.mjs` 산출물의 source snapshot/runtime identity,
   artifact 길이·SHA-256을 검증하고 실제 WASM을 한 번 컴파일한다.
2. 같은 compiled module을 사용하지만 독립 linear memory와 registry를 가지는
   owner 하나와 실제 Node worker 두 개를 만든다. worker에는 bindings 경로,
   compiled module, runtime identity만 초기 전달한다.
3. 실제 WASM의 embedded signed catalog로 다섯 pack의 profile·길이·digest를
   확인하고 full pack은 owner 한 곳에만 admission한다. peer에는 256KiB 이하
   seed와 query/reply만 보내며 peer cache 예산은 각각 1MiB다.
4. 공통 command ingress의 CPU solver를 사용해 no-asset 기준, full owner,
   cold peer의 exact fallback, reply import 뒤 재실행, 제거 뒤 재실행을 대조한다.
   SRS+에서는 두 peer 각각 취소·정상 재실행도 대조한다.
5. complete flag, 전체 solution identity 순서·집합 digest, coverage denominator,
   coverage count·확률, 해법별 확률과 실제 응답 runtime identity를 확인한다.
   실제 peer query/reply가 한 번도 발생하지 않으면 실패한다.
6. 성공·실패와 관계없이 이 테스트가 만든 worker 두 개만 종료한다.

입력은 모든 profile에 대해 `clearra pc --lines 4 --board-mask 0x3c0f03c0f
--height 4 --pieces 6 --patterns P7 --hold empty --objective unique --count unique
--solution-probabilities --backend cpu --workers 1 --rule PROFILE --no-tablebase
--no-legal-board`이며 조건부 도달성만 독립 on/off한다.

각 realm의 solver 요청 worker는 1이다. 두 Node worker의 realm 격리·ABI 증거를
11-worker 제품 탐색이나 병렬 성능 증거로 표현하지 않는다. pack은 실행 중
profile별로 따로 admission·제거하며 다른 profile의 자료로 대체하지 않는다.
cold miss의 exact fallback 결과 일치와 bounded reply 교환은 relation hit rate,
BuildUp 성능 채택 또는 128MiB 실제 OS peak의 증명이 아니다.

## 관측 상태

| 검사 | 관측 상태 | 권위 범위 |
| --- | --- | --- |
| 기준 SHA 비게시 CI | 다섯 작업 성공 | 이전 source의 focused feedback |
| 새 비게시 CI source 계약 | Node 9개 통과 | 새 job의 출력 경로·source identity·단일 build·기존 자료만 사용 |
| 새 실제 WASM realm 테스트 | 1개 통과, skip 0; 47개 정상 요청과 2개 취소 | 다섯 실제 signed pack의 full-owner/cold-peer/reply-import/remove 결과 parity |
| 브라우저 host/worker/OPFS 경계 | Open | 브라우저 연결 도구의 kernel asset 준비가 `os error 3`으로 실패하여 미실행 |

브라우저 제어 스킬의 초기 연결 단계가 실패했다. 선택된 브라우저를 다른
자동화 도구로 우회하거나 기존 4194 세션을 변경하지 않았다. Node의 실제 WASM
realm 검증은 보완 증거이며 production browser worker entrypoint, verifier pool,
host broker·OPFS·브라우저 정책의 검증을 대신하지 않는다.

작은 기존 입력에서 관측한 결과이며 전체 P7P4 qualification 값이 아니다.
각 profile의 두 peer 모두 baseline과 일치했고 removal 뒤에도 같은 결과였다.

| Profile | 전체 solution identities | bounded peer query/reply batches | 결과 parity |
| --- | ---: | ---: | --- |
| SRS+ | 246 | 6 | 동일; 두 peer 각각 취소·재실행 포함 |
| SRS | 245 | 4 | 동일 |
| SRS-X | 289 | 4 | 동일 |
| Jstris 180 | 246 | 4 | 동일 |
| no-kick | 175 | 4 | 동일 |

이 batches는 실제 공개 query/answer/import ABI 호출 수다. 실제 relation hit
수나 전체 solver worker 수를 뜻하지 않는다. 각 profile에서 교환이 없으면
실패하도록 별도 assertion도 유지한다.

초기 테스트가 cancel 뒤 이미 해제된 job을 다시 advance하여
`E_WASM_WORKER_JOB_MISSING`으로 실패했다. source를 확인해 실제 `WasmJobRunner`와
같이 cancel→drain으로 고쳤다. 단일 cancelled event와 `scope_released=true`,
terminal result 부재를 확인하며 그 뒤 정상 재실행까지 검증한다. 제품의 취소
계약이나 구현을 바꾸거나 실패를 success로 무시하지 않았다.

## 실행·CI 소유권

WASM은 `_local/artifacts/v081-browser-peer-smoke/wasm`에 일반 제품 producer로
한 번 생성한다. profiling/benchmark feature나 benchmark provenance는 사용하지
않는다. 실제 자료는 기존 `_local/artifacts/v081-peer-signed-smoke`의 다섯
`conditioned-PROFILE.cllr`를 재사용한다. source snapshot에 포함된 Rust/Cargo/
producer 파일은 빌드 도중 변경하지 않는다. raw 산출물은 Git에 넣지 않는다.

첫 로컬 컴파일은 Windows 실행 정책의 `os error 4551`로 중단됐다. 사용자 정책에
따른 수동 재시도 한 번만 수행했으며 자원·보안 정책을 변경하거나 WSL·자동
재시도를 사용하지 않았다. 이 제한을 성공 실행으로 집계하지 않는다.

재시도에서 Cargo와 wasm-bindgen, artifact staging 및 compile-source 계약 대조는
완료됐다. WASM은 23,640,610 bytes이며 SHA-256은
`391a6fc4d0711c35de4453ae9ce71c10c8cf4aedb0e8caea267f4a44360c1bef`다.
하지만 관리기는 `apps/`·`scripts/`의 테스트 입력이 빌드 중 변경된 것을 확인해
incremental seed 봉인을 거절했다. 영수증
`1790530394157446100-40444-runtime.json`은 return code 1, reason `nonzero`,
owned tree stopped, descendant 0이며 메모리 pressure event 5회를 기록한다.
이를 정상 build receipt나 캐시 재사용·acceptance 권위로 바꾸지 않는다.

테스트는 그 산출물의 compile-source 계약과 바이트 해시 및 실행 identity가 현재
변경 없는 Rust source와 일치하는 것을 독립 확인한 뒤 실행했다. 실제 realm
집중 검증의 정상 영수증 `1790531834903064000-12584-runtime.json`은 return code 0,
reason `normal`, owned tree stopped, descendant 0, memory pressure event 0이다.
최종 per-profile 교환 assertion과 CI source 계약을 함께 실행한 10개 테스트도
skip 없이 통과했다. 영수증은 `1790532023248761900-33396-runtime.json`이다.
감독의 전체 Node/WASM runtime memory를 공유 자료 128MiB gate나 benchmark로
해석하지 않는다. 같은 산출물을 재빌드 없이 사용하며 source guard를 완화하지 않는다.

새 비게시 `wasm-realms` job은 기존 다섯 작업과 독립 실행하며 pinned Rust,
managed wasm-bindgen, manager의 명시 출력 경로 검증 뒤 일반 WASM을 한 번만
만든다. 그 exact source의 manifest와 변경 없는 다섯 Release pack을 지정하여
테스트를 실행한다. 새 qualification receipt·Release·promote를 만들지 않고
실패를 success로 바꾸지 않는다. 입력이 없는 일반 Node 실행의 명시 skip은
실제 자료를 지정한 CI 통과를 대신할 수 없다.

## 남은 경계

- 실제 browser host/pool의 seed·reply·취소·세션 교체 및 다운로드 readback
- legal-board negative synopsis의 실제 browser 분산 실행
- Desktop·Discord의 실제 qualified data-layer smoke와 전체 surface parity
- strict lint, active-session 실제 peak와 성능 gate
- exact source acceptance, 배포·rollback readback

이전 계획과 자격 완료 자료는 보존하며 위 항목을 좁은 realm 증거로 닫지 않는다.
벤치마크는 계속 다음 단계로 미룬다.

# v0.8.1 실제 App 실행의 가속기 결과 동등성 집중 검증

## 범위와 기준선

기준 소스는 후보 브랜치 `codex/v081-selective-source-ci-20260927`의
`108a4387d01d4e69a75fa04320b443aeda88bf94`다. 그 소스의 비게시 CI
[`36333449053`](https://github.com/daejunnom/Clearra/actions/runs/36333449053)는
Core·native products·WASM ABI·surfaces 네 작업 모두 성공한 것을 확인했다.
아래 새 테스트와 CI 변경의 성공을 이전 실행으로 대신하지 않는다.

이번 단계는 입력 옵션 전달 다음의 실제 App/Core 실행과 reducer 결과 검증이다.
벤치마크·ABBA·P7P4·시간 비교·자료 재생성·전수 재자격·TB 업그레이드·main 병합·
4194 변경·배포는 수행하지 않는다. 기존 primary/mixed 작업트리를 수정하지 않는다.

## 실제 제품 계약을 사용하는 독립 집중 target

`clearra-app/tests/exact_accelerator_product_execution.rs`는 production
`AppContext`와 `AppCoreExecutorService::wasm_cpu()`를 실행한다. fake Core 결과나
대체 solver를 사용하지 않는다. 큰 App unit-test 실행 파일 대신 작은 integration
target을 사용하고, `--no-default-features --features parallel`로 제품 정책을 검증한다.
`local-search-ab` 전역 토글이나 benchmark 전용 제한·타이머는 사용하지 않는다.

각 입력에서 `(legal-board, conditioned-reachability)`의 네 가지 조합을 비교한다.

| 작은 입력 | 확인한 실제 결과 |
| --- | --- |
| 1~6L, 각 행 `0x3f0`, 고정 I 큐, 최소 해법 | 전체 candidate map·coverage rows·required universe·source set digest·최소 cardinality·첫 canonical 선택·완결성 |
| 1L 동일 필드, score-minimals | pattern별 최고 점수·eligible candidate map digest·score eligibility digest·canonical 최소 집합·완결성 |
| 4L 높이의 빈 base, 한 개 I로 만든 `[0xf,0,0,0]` Build target | source candidate 수·normalized source set digest·확률 `1`·canonical 집합·전체 portfolio map·완결성 |
| 1L PC chance | covered/total pattern 수·coverage bitset·probability bits·materialized probability mass·완결성 |
| 1L 전체 PC replay | 전체 witness 배열·canonical witness·ordering·terminal empty board·한 줄 삭제·완결성 |

최소 해법의 공개 typed 입력은 `CountUnique`이며 제품이 내부 full-source
정규화를 소유한다. score-minimals는 `CountAll`, 고정 max-pattern 계약 및 단일
retained trace를 사용한다. Build v2는 자체 typed request가 계약을 소유하므로 PC
제품용 capability ingress wrapper를 붙이지 않는다. 테스트를 이 경계에 맞췄으며
기존 계약을 완화하거나 제품 코드·결과 의미를 변경하지 않았다.

비교에서 제외하는 것은 요청/telemetry 식별자뿐이다. whole-response를 임의로
scrub해 비교하지 않고 candidate key와 coverage universe 등 의미 필드를 명시한다.
최소 해법의 실제 Core telemetry도 대조하여 request flag가 정책과 session snapshot까지
도달함을 확인한다. 요청이 꺼져 있으면 설치 여부와 관계없이 snapshot이 비활성이다.

## 실제 서명 자산과 수명

일반 테스트는 자산 없는 exact fallback을 확인한다. 별도 ignored 테스트는 명시적으로
다운로드한 기존 SRS+ 두 파일만 읽는다.

- `legal-board-data-v081-20260924-rc1`의 `legal-board-srs-plus-v2.cllb`
- `conditioned-data-v081-20260924-rc1`의 `conditioned-srs-plus.cllr`

이미 확인한 `_local/artifacts/v081-peer-signed-smoke` 루트를 재사용한다. 새 generation,
서명 키, catalog, payload 또는 qualification receipt는 만들지 않는다.
production embedded catalog와 opaque authority로 실제 길이·digest·profile·generation을
검증하고 동일 runtime registry에 설치한다. 각 실행 전후 identity가 유지되고 마지막
remove 후 두 active identity가 모두 없어짐을 확인한다. 이는 process-local 설치이며
사용자 persistent store를 삭제하거나 변경하지 않는다.

조건부 snapshot이 켜져 실제로 pinned된다는 것과 relation hit는 별개다. 이 작은
입력들은 harddrop/exact fallback을 사용할 수 있고, legal-board의 empty-origin 정확한
4L domain도 검증하지 않는다. 이번 smoke로 prune 효과나 hit rate를 주장하지 않는다.
앞 단계의 다섯 signed pack 560개 solver/context 차분과 전체 legal-board 자료 자격은
기존 별도 증거로 보존하며 재실행하지 않는다.

## 관측 결과와 CI

| 검사 | 결과 | 권위의 한계 |
| --- | --- | --- |
| 실제 App, 자산 없음 | 집중 테스트 1개 통과, 작은 제품 실행 40개 | 기준 exact fallback과 reducer의 좁은 동등성 |
| 실제 App, SRS+ 서명 pair 설치 | 집중 테스트 1개 통과, 동일 작은 제품 실행 40개 | 실제 admission·snapshot·identity·remove; profile 전수 제품 검증 아님 |
| 비게시 CI 계약 | Node 7개 통과 | CI 실행 성공을 대신하지 않음 |
| formatting·diff whitespace | 통과 | runtime/release 자격 아님 |

최종 직접 test-executable 감독 영수증은
`1790528209074789400-25060-runtime.json`과
`1790528218660002900-41900-runtime.json`이다. 두 실행 모두 return code 0,
`reason=normal`, 종료 시 descendant 0, memory-pressure event 0을 확인했다.
감독 영수증을 active-session 공유 peak 128MiB 또는 성능 benchmark 증거로 사용하지 않는다.

Cargo의 test executable 컴파일은 끝났지만 별도 finite descendant drain이
`E_CLEARRA_PROCESS_TREE_NOT_STOPPED`로 끝났다. 감독 경고를 지우거나 정상 빌드 종료로
기록하지 않는다. 이어서 Cargo가 출력한 정확한 실행 파일을 같은 감독 정책으로
직접 실행해 위 정상 테스트 증거를 얻었다. 감독 한도 변경·OOM 자동 재시도·외부
프로세스 종료는 하지 않았다. 기존 strict-Clippy 미완료 항목도 이 작업에서 닫지 않는다.

후보 전용 CI는 일반 실행과 signed 실행에서 동일 target·feature를 사용해 실행 파일을
재사용한다. native-products job에서 위 두 immutable 파일만 명시 다운로드한다.
typed/native check를 통과하면 독립 집중 검증을 계속하되 실패를 success로 바꾸지 않는다.
게시·generation promote·main 변경·Cloud traffic 변경 단계는 없다.

## 계속 Open인 경계

실제 CLI persistent lifecycle, Web/Desktop/Discord presenter의 결과 readback,
여러 WASM realm의 signed asset hit·취소·교체, 다중 candidate의 lazy tie와 copy/page
전체 의미, strict-lint, 공유 peak/성능 gate, exact-SHA acceptance와 배포·rollback은
계속 Open이다. 1~6L의 작은 실제 App 실행을 모든 초기 필드·queue·profile·worker의
전체 parity로 승격하지 않는다. 벤치마크는 다음 단계, v0.9.0 업그레이드는 동결한다.

## 후속 실제 CI와 요청 워커 대조 확대

exact `e1db5ba87b86df98fb6e8ca39b246c0596d58e5f`의
[run 36352731057](https://github.com/daejunnom/Clearra/actions/runs/36352731057)에서
일반 App 실행 step과 SRS+ signed pair 설치 후 실행 step의 success를 확인했다.
이 source에는 Setup-score의 실제 Build coverage·PC continuation·ranking 및
요청 worker 1·2·11의 public payload 비교도 포함된다. 완료된 step의 상태와
아직 진행 중인 전체 native job의 상태를 구분하며, 상세 TAP는 job 완료 후
별도로 대조한다. 같은 실행의 `surfaces` 실패는 유지한다.

현재 source를 다시 확인한 결과 최소 해법의 작은 입력은 이미 `1..=6`으로
실행하고 있었지만 요청 worker는 1로 고정되어 있었다. 후속 수정은 이 기존
줄 수 범위를 유지하며 최소 해법과 score-minimals에 요청 worker 1·2·11 및
두 가속기의 네 on/off 조합을 모두 전달한다. 자산 없음/서명 pair 설치를 검증하는
기존 단일 integration target의 두 테스트와 exact pipeline을 그대로 사용한다.

- minimum: 기존 필드가 있는 각 1~6L에서 요청 worker·가속 정책 12개 조합
- score-minimals: 기존 1L 입력에서 같은 12개 조합
- 기존 1-worker/off-off baseline은 재실행하지 않고 기준 결과를 재사용
- source solution 집합, candidate map, coverage universe, 최소 개수, canonical
  선택 순서 및 score eligibility를 기존 실제 typed result로 비교
- 요청 worker와 실제 사용 worker를 동일시하지 않으며 parallel 성능·전체
  profile/domain 또는 128MiB 공유 peak를 이 검사로 증명하지 않음

확대한 target은 렌더 미포함 `parallel` 타입 검사 및 formatting/diff 검사를
통과했다. 감독 receipt `1790546717418435200-43044-runtime.json`은 정상 return 0,
pressure 0, descendant 0, tree stopped를 확인했다. 실제 확대 행렬의 실행은
후속 exact-source Linux CI에서 확인할 Open 항목이다. Windows 실행 정책 우회,
benchmark/ABBA, 자료 재생성 및 v0.9.0 변경은 하지 않는다.

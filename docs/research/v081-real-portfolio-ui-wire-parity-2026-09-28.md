# v0.8.1 실제 최소 집합의 lazy 페이지·전체 집합 복사 검증

## 범위와 기준선

후보 브랜치는 `codex/v081-selective-source-ci-20260927`이며 기준 소스는
`e4eef736bd201706a8333af52c2c8a3899d0fe38`다. 해당 소스의 비게시 CI
[`36335286645`](https://github.com/daejunnom/Clearra/actions/runs/36335286645)는
Core·native products·WASM ABI·surfaces 네 작업 모두 성공한 것을 확인했다.
이전 실행의 성공을 아래 새 테스트나 새 CI 작업의 성공으로 대신하지 않는다.

이번 단계는 작은 실제 제품 결과를 공통 command ingress, production
`WasmCommandRuntime`/App/Core, lazy page store, 브라우저용 JSON serializer,
UI validation/CTK3 export에 연결하는 기능 검증이다. 벤치마크·ABBA·P7P4,
시간 비교, qualified 자료 재생성·재자격, TB 업그레이드, main 병합, 4194 교체,
게시·배포는 수행하지 않는다. production solver와 공개 결과 의미는 변경하지 않는다.

## 실제 입력과 확인한 의미

`crates/clearra-wasm/tests/exact_accelerator_portfolio_wire.rs`는 fake Core나
합성 후보를 넣지 않고 실제 공통 CLI/Web command text로 CPU 제품을 실행한다.
요청 worker는 명시적으로 1이며 여러 WASM realm이나 병렬 성능 증거가 아니다.
테스트 target의 `webgpu-search` feature는 기존 WASM ABI job과 dependency feature를
맞추기 위한 것이며, 입력은 CPU로 고정하고 GPU나 TB를 사용하지 않는다.

| 실제 작은 입력 | 확인한 의미 |
| --- | --- |
| 빈 2L, `IIOOO`, hold 없음, `pc minimals` | 실제 source candidate 4개, 최적 cardinality 1, canonical 첫 집합 하나를 먼저 응답하고 나머지 3개를 bounded slice로 lazy 열거 |
| 빈 Build base, target `0xf`, 높이 4, 고정 I, `build pinned-minimals` | 원본·대칭 필드 `0xf`/`0x3c0`를 CTK3 필수 목록으로 지정, 실제 source candidate 2개가 모두 필수이며 cardinality 2의 집합 전체를 복사 |

각 입력을 legal-board/조건부 도달성의 네 on/off 조합으로 실행하여 총 8개
실제 요청의 candidate key, canonical outer 순서, member key와 CTK3/Fumen
복사 payload가 동일함을 확인한다. 이 target은 서명 자산을 설치하지 않으므로
optional 자산의 unavailable/fail-open 경계 검증이며 hit/prune 증거가 아니다.
실제 SRS+ signed pair를 설치한 App 검증은
[앞 단계 기록](v081-real-app-accelerator-result-parity-2026-09-28.md)의 별도 증거로 보존한다.

PC의 네 번째 집합을 조회하면 보관 한도 3개 때문에 첫 집합이 evict된다.
첫 집합의 replay를 취소하면 retained cache가 변하지 않고, 이어서 정상적인
bounded slice 재조회가 완료되면 최초 페이지와 native copy payload가 정확히
복원된다. store의 전체 known-count high-water도 역행하거나 중복 증가하지 않는다.
각 집합은 immutable metadata와 canonical candidate 순서를 유지한다.

## Rust → 실제 UI 함수의 경계

ignored producer는 같은 8개 실제 요청과 assertion을 실행한 뒤 명시적으로
검증한 `_local/artifacts/v081-product-page-smoke/portfolio-wire-smoke.json`에만
자료를 쓴다. raw 자료는 Git이나 `docs/research/`에 넣지 않는다.

`packages/clearra-ui/test/realPortfolioWire.test.mjs`는 이 실제 자료를 읽어
production final-result/page/artifact validator와 기존 whole-set export source를
실행한다. UI가 CTK3로 복사한 필드의 높이·색·페이지 순서를 native copy와
대조하고, 다른 결과로 교체되거나 사용자 취소가 발생하면 cached copy도 거절한다.
Build의 2개 member 모두 유지되며 PC 동률 집합에 중복이 없다.

입력이 없는 일반 UI 실행은 이 테스트를 명시적으로 skip한다. 이번에는 실제
Rust producer 자료를 지정하여 **skip 없이 통과**했다. 작은 집합이므로 실제
100-member 경계를 넘었다고 주장하지 않는다. 별도의 기존 exporter 집중 테스트는
205-member fixture로 표시 100개와 현재 선택 집합 전체 복사의 분리를 확인한다.
이 기존 fixture를 실제 solver에서 나온 205개 결과로 표현하지 않는다.

## 관측 결과와 감독 경계

| 검사 | 관측 결과 | 한계 |
| --- | --- | --- |
| 실제 Rust product/page/copy | 집중 테스트 1개 통과, 실제 작은 요청 8개 | native 실행, 자산 미설치 |
| 실제 UI용 Rust producer | ignored 테스트 1개 통과, 같은 8개 요청을 새로 생성 | 기능 자료일 뿐 benchmark/qualification receipt 아님 |
| 실제 자료를 읽은 UI 소비자 | 1개 통과, skip 0 | UI 함수 검증이며 실제 브라우저·clipboard 권한 검증 아님 |
| 기존 전체 집합 exporter | 11개 assertion/test 통과 | 일부는 합성 multi-page fixture |
| 비게시 CI 계약 | Node 8개 통과 | 새 원격 CI 성공을 대신하지 않음 |
| `cargo fmt --all --check`, diff whitespace | 통과 | release/성능 자격 아님 |

Rust test executable을 직접 실행한 감독 영수증은
`1790529722894026800-30336-runtime.json`이며 UI 자료 producer는
`1790529735039432200-38572-runtime.json`이다. 두 실행 모두 return code 0,
`reason=normal`, process tree stopped, descendant 0, memory-pressure event 0이다.
이 작은 테스트의 메모리·실행시간을 제품 active-session peak나 성능 gate로 사용하지 않는다.

초기 테스트 코드의 enum 비교, 명령 문자열 구분자와 Build registry에 없는
`--include-mirror`를 바로잡았다. registry나 제품 계약은 확장하지 않았다.
직접 실행의 기본 테스트 스택에서는 overflow가 발생했으며, CI에 이미 선언된
`RUST_MIN_STACK=16777216`을 동일하게 지정한 실행에서 정상 종료했다.
감독 profile·메모리 한도를 변경하거나 OOM을 자원 증설로 재시도하지 않았다.
이 결과로 실제 WASM stack 경계까지 증명했다고 주장하지 않는다.

Cargo는 정확한 test executable을 생성했지만 그 컴파일 감독의 descendant drain은
`E_CLEARRA_PROCESS_TREE_NOT_STOPPED`로 끝났다. 경고를 정상 빌드 종료로 바꾸지
않으며 위 직접 테스트의 정상 영수증과 구분한다. 외부 프로세스를 종료하지 않았다.

## 비게시 CI 전달과 남은 항목

기존 WASM ABI job이 해당 native functional producer를 한 번 실행하고
source SHA/run ID/attempt에 결박된 이름의 artifact로 정확한 JSON 파일 하나를
전달한다. retention은 3일이다. 별도 `product-wire-ui` job은 그 실행의 자료만
내려받고 필수 파일 존재를 확인한 뒤 실제 UI 소비자를 실행한다. UI job에서
Rust를 다시 컴파일하지 않고 기존 독립 surfaces job도 producer를 기다리지 않는다.
앞선 독립 ABI 실패가 있어도 producer 검증을 실행할 수 있지만 실패를 success로
바꾸지 않는다. 게시·릴리스 권위나 generation promote는 만들지 않는다.

State-major memo의 작은 입력 flat/큰 live memo 승격이라는 기존 adaptive 기본값은
유지한다. 기준 소스 CI의 `v081_` 필터는 작은 입력 비승격·정확한 전체 key 이전·
실패 시 flat 보존 등 기존 focused 회귀를 포함하지만, 성능 비회귀를 증명하지는 않는다.

실제 CLI persistent lifecycle, Web/Desktop/Discord presenter readback, 여러 WASM
realm의 signed asset hit/cancel/교체, 전체 profile·worker·candidate universe,
strict lint, active-session 공유 peak, 성능 gate, exact-SHA acceptance와
배포·rollback은 계속 Open이다. 벤치마크는 다음 단계, v0.9.0 업그레이드는 동결한다.

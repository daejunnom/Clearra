# 빌드 중복과 SIGKILL 진단: 2026-11-23

## 1. 확인 범위와 현재 결론

초기 Full4의 기존 경로와 symbolic 경로, 이후 compatibility filter, row-prefix,
미노 종류별 cache 수명 변경을 적용한 세 symbolic 실행까지 모두 SIGKILL로 종료됐다.
완성된 결과를 비교하지
못했으므로 탐색 속도 향상, 완료 작업의 메모리 절감, 전체 해법 집합의 동등성을
주장하지 않는다. 종료 원인의 우선 가설은 메모리 압력이지만, 커널 OOM과 외부
프로세스가 보낸 SIGKILL을 구분할 호스트 증거는 확보하지 못했다. 두 실행은 자원을
바꿔 재시도하지 않았다. 이후 실행은 새 구조와 종료 계측을 적용한 별도의 source
revision이며, 자원 제한이나 worker 수를 낮춘 동일 코드 재시도가 아니다.
최종 후보는 rank 2 완료 후 rank 3의 `movement`에서 종료됐다. 전체 Full4 해법·coverage
검산은 완료하지 못했으므로 Full6 확대 판단은 충족되지 않았다(`Full6 gate = false`).

빌드에서는 별도의 구체적 원인을 확인했다. executor의 example을 빌드하면
dev-dependency인 FFI `test-support`가 합류해, 직전에 만든 같은 executor 라이브러리를
그대로 재사용할 수 없었다. 이를 피하도록 실행 소스를 공유하는 별도 tools 패키지
manifest를 추가했다. 검색 알고리즘과 release 최적화 수준을 낮추는 변경은 아니다.
통합 후 shipping 설정 빌드는 timing HTML 기준 `146.4s`에 완료됐고, 같은 소스와
환경의 무편집 warm build는 62개 unit을 모두 재사용해 Cargo 기준 `0.11s`에 끝났다.
이 두 값은 cache 재사용 여부가 다른 관측이며, 분리 전후의 속도 개선율은 아니다.
이후 미노 종류별 cache 수명을 정리한 최종 후보의 세 binary shipping 빌드는
`121.2s`에 완료됐다. 소스와 target 집합이 다른 각 빌드의 조건은 아래에 따로 기록한다.

이 문서는 실제 로그·Cargo fingerprint·timing HTML·ELF 메타데이터와 읽기 검토를
구분한다. 새 실행기 통합 뒤 release 및 warm build의 측정은 아래 표에 별도로
기록한다. 아래에는 종료된 다섯 실행의 관측만 포함하며, CI 실행·조회·대기나 Full6
탐색은 이 조사에 포함하지 않는다.

## 2. 확인한 빌드 환경

| 항목 | 확인값 |
| --- | --- |
| Rust | `1.98.1 (48a229cea 2026-09-01)` |
| host/target | `x86_64-unknown-linux-gnu` |
| LLVM | `22.1.8` |
| 저장소 release | `lto = "thin"`, `codegen-units = 1`, `strip = "symbols"` |
| 저장소 build cache | `build/cargo/default` |
| 저장소 incremental 설정 | `.cargo/config.toml`: `build.incremental = true` |
| workspace resolver | `2` |
| 기존 Full4 실행파일의 실제 linker | `LLD 22.1.8` |

Rust/LLVM 값은 기존 Cargo cache의 `.rustc_info.json`과
[`rust-toolchain.toml`](../../rust-toolchain.toml)을 대조했다. 실제 linker는
`readelf -p .comment build/cargo/default/release/examples/symbolic_family_compare`로
확인했다. 이 파일에는 `Linker: LLD 22.1.8`, GCC 14.2.0, rustc 1.98.1이 기록돼 있다.
toolchain에는 `rust-lld`와 `gcc-ld/ld.lld`도 존재한다. 따라서 현재 바이너리에
GNU ld에서 LLD로 바꾸는 개선을 새로 적용했다고 말할 수 없다.

`sccache`, `ccache`, `mold`, `clang`은 현재 PATH에서 발견되지 않았다. 저장소 Cargo
config에는 wrapper나 linker override가 없다. 이것은 전역 모든 설치 위치를 조사한
결과가 아니며, 설치나 wrapper 전환에 따른 속도 개선을 측정하지도 않았다.

기존 Full4 비교용 바이너리는 최초 빌드부터 `opt-level=3`, `lto=false`,
`codegen-units=16`, `incremental=false`를 명시한 진단용 설정이었다.
저장소의 shipping release 설정을 바꾼 것이 아니다. 이 바이너리의 시간과 이후
thin-LTO/codegen-units-1 빌드를 같은 조건의 전후 값으로 묶지 않는다.
상세 source tree와 바이너리 지문은
[`full4-symbolic-family-comparison-2026-11-23.md`](full4-symbolic-family-comparison-2026-11-23.md)에 있다.

## 3. 라이브러리와 example 사이의 실제 중복 unit

[`clearra-core-executor/Cargo.toml`](../../crates/clearra-core-executor/Cargo.toml)은
`clearra-core-ffi`를 normal dependency로 선언하고, dev-dependency에서는 같은
패키지의 `test-support` feature를 추가한다. resolver 2는 일반 dependency 빌드에서
dev feature를 제외하지만, tests/examples처럼 dev-dependency가 필요한 target을
선택하면 해당 feature가 활성화된다.
근거: [Cargo feature resolver 2](https://doc.rust-lang.org/cargo/reference/features.html#feature-resolver-version-2).

실제 release cache에는 다음 두 FFI unit이 함께 존재한다.

| fingerprint 디렉터리 suffix | FFI features |
| --- | --- |
| `edaa2b1a7790e1f8` | `default` |
| `6af694462d85eae2` | `default, test-support` |

두 JSON의 rustc, profile, target, config 및 FFI의 하위 dependency 지문은 같다.
executor library도 `15113b19097cb058`과 `5f512d40f6383076` 두 unit이 존재한다.
이 둘의 executor features는 모두
`default, parallel, placement-dependency-pc, symbolic-family`이고 profile은
`8520514848050168305`다. dependency 목록의 차이는 `clearra_core_ffi`의 지문이다.

따라서 `--lib`를 먼저 빌드하고 `--example symbolic_family_compare`로 전환했을 때의
재컴파일은 Cargo cache가 무작위로 사라진 현상으로 설명할 필요가 없다. 선택한 target이
dependency feature 집합을 바꿨다. 실제 첫 example 로그에도 FFI, executor,
`clearra-accelerator-product-host`, `clearra-fumen`의 컴파일이 나타난다.

| 기존 관측 | 결과 |
| --- | --- |
| release library | Cargo 표시 `1m 06s`, 성공 |
| 첫 example | FFI/executor 재컴파일 후 JSON macro의 배열 반복 문법 오류 |
| 해당 문법 수정 후 같은 설정 example | `2.80s`, 성공 |

첫 example은 실패한 빌드이므로 성공한 최종 바이너리의 완료 시간으로 취급하지
않는다. `2.80s` 역시 이미 준비된 dependency를 활용한 수정 빌드이며 cold build가 아니다.

## 4. 실제 `--timings`: executor test unit이 긴 구간

첫 core timing은
`build/cargo/default/cargo-timings/cargo-timing-20261123T085040570Z-b98240691fcea4a2.html`
의 `UNIT_DATA`를 처음 읽은 뒤, Rust 추출기
`_local/artifacts/cargo-timings-extract-20261123.rs`로 원문을 다시 읽어 검증했다.
첫 test 및 아래 release/warm 빌드의 HTML 네 개를 재추출한 결과는
`_local/artifacts/cargo-timings-reviewed-20261123.json`에 보존했다. 이 재검증은
기존 로그를 읽는 작업이며 Cargo 빌드나 테스트를 추가 실행한 것이 아니다.

| 항목 | 관측값 |
| --- | ---: |
| Cargo 완료 시간 | `47.47s`; HTML 표시는 `47.5s` |
| 전체 unit | 62 |
| Fresh / Dirty | 39 / 23 |
| jobs / ncpu | 9 / 9 |
| 관측 max concurrency | 6 |
| executor test lib | 시작 `11.18s`, duration `36.29s` |
| clearra-coverage | 시작 `1.03s`, duration `10.15s` |
| coverage frontend / codegen | `3.47s` / `6.68s` |
| FFI, `default,test-support` | 시작 `7.71s`, duration `1.26s` |

coverage가 끝나는 `11.18s`에 executor test unit이 시작하고, 이 unit이 `47.47s`까지
이어진다. 주된 빌드 critical path는 executor다. executor unit의 `sections`는 `null`
이므로 `36.29s` 중 frontend, codegen, linker의 시간을 분리할 수 없다. 이를 linker만의
병목이라고 해석하지 않는다. dependency들의 구간은 서로 겹치므로 duration 합계를
전체 wall time에 더하지 않는다. max concurrency 6도 worker 수를 6으로 제한했다는
뜻이 아니다.

algebra 3개가 통과한 빌드의 Cargo 시간은 `49.83s`였다. 이어 같은 binary의
solver-equivalence 4개는
Cargo 준비 `0.08s`, 테스트 `0.08s`로 통과했다. 이는 기능 회귀이며 작은 입력의
성능 비교가 아니다.

### 이전 `19.81s`와 직접 비교할 수 없는 이유

| 항목 | 이전 membership test build | 새 algebra test build |
| --- | --- | --- |
| Cargo 시간 | `19.81s` | `47.47s` |
| executor fingerprint | `d4832b55e11405e3` | `988fb1f0f1f444b4` |
| profile fingerprint | `6828309776290197780` | `13188599775219529741` |
| features | `default, placement-dependency-pc, symbolic-family` | 앞의 집합에 `parallel` 추가 |
| incremental 정책 | 저장소의 활성화된 incremental 경로 | 명시한 `CARGO_INCREMENTAL=0` |
| test debug | `CARGO_PROFILE_TEST_DEBUG=0` | `CARGO_PROFILE_TEST_DEBUG=0` |

같은 rustc/config/rustflags에서 직접 dependency 21개 중 workspace/path dependency
19개의 지문이 달라졌고, registry dependency인 `ed25519_dalek`과 `sha2` 두 개만 같다.
이는 incremental을 끄면서 path unit의 profile이 달라진 관측과 일치한다. source와
feature 범위도 바뀌었으므로 이 두 값의 차이를 새 알고리즘이나 빌드 변경 하나의
속도 효과로 계산하지 않는다.

Cargo incremental은 workspace/path dependency에 적용되며 `CARGO_INCREMENTAL`은
profile과 `build.incremental` 설정을 전역으로 override한다. 반복 개발 빌드는
profile·feature·incremental 정책을 고정해야 재사용 효과를 평가할 수 있다.
근거: [Cargo profiles: incremental](https://doc.rust-lang.org/cargo/reference/profiles.html#incremental),
[Cargo build.incremental](https://doc.rust-lang.org/cargo/reference/config.html#buildincremental).

## 5. 적용한 빌드 구조 변경과 실제 shipping 계측

[`tools/clearra-symbolic-bench/Cargo.toml`](../../tools/clearra-symbolic-bench/Cargo.toml)은
두 binary가 기존 `symbolic_family.rs`와 `symbolic_family_compare.rs` 소스를 그대로
사용하게 한다. executor는 이 패키지의 normal dependency이므로 executor 자체의
dev-dependency가 실행기 빌드에 합류할 필요가 없다. FFI의 `test-support`를 제품
feature로 승격해 중복을 없애는 방식은 사용하지 않았다.

실행기의 `symbolic-family`, `parallel`, `placement-dependency-pc` feature는 실제
executor feature로 전달한다. 기본 실행은 executor의 default feature identity도
보존하므로 `cfg!(feature)` 보고와 요청한 feature 집합이 맞는다. 기존 example 진입점은
그대로 남는다. workspace에 통합된 패키지를 명시적으로 선택하는 것이 이 변경의
사용 경로다. 여러 package를 동시에 선택한 build의 feature unification까지 없어진다는
주장은 하지 않는다.

두 manifest 위치가 모두 저장소에서 두 단계 아래이므로 기존
`CARGO_MANIFEST_DIR/../..`의 artifact root 계산도 유지된다. release opt-level,
thin LTO, codegen-units, 검색 worker 정책, 검색 계산은 이 manifest 변경에서 바뀌지 않는다.

### 측정 조건과 provenance

root의 단일 supervised build에서 최종 target을 직접 선택했다. 선행 `--lib` build를
추가하지 않았으며, 같은 명령을 소스 편집 없이 한 번 더 실행해 warm 상태를 확인했다.

```sh
cargo build --locked -p clearra-symbolic-bench --release \
  --bin symbolic_family_compare --timings -vv
```

두 compile invocation에는 `SOURCE_COMMIT=1fe73ed88fa350ef0ebd9ae9e17281061fc2ff54`,
`SOURCE_TREE=81faa07b0d8cb140813ecc1c12782571e6d40cda`가 동일하게 전달됐다.
commit은 작업 기준 revision이며, source tree는 그 위에서 빌드한 변경 내용을 식별한다.
`build/cargo/default`를 유지했고 Cargo cache를 지우지 않았다. 첫 측정에서 62개
unit이 모두 Dirty였어도, 저장소·다운로드·filesystem cache까지 비운 clean-from-scratch
benchmark는 아니다. 새 코드와 shipping profile에 대한 rebuild로 해석한다.

실제 `rustc` 명령은 라이브러리에 `opt-level=3`, `linker-plugin-lto`,
`codegen-units=1`, `strip=symbols`를, 최종 binary에 `opt-level=3`, `lto=thin`,
`codegen-units=1`, `strip=symbols`를 전달했다. FFI, executor 및 binary에는 모두
`-C incremental=.../build/cargo/default/release/incremental`이 있었다. 이 실행에서
incremental을 비활성화하거나 release 최적화를 낮추지 않았음을 실제 인자로 확인했다.
registry dependency까지 동일한 incremental 경로를 사용한다고 확대하지 않는다.

| 항목 | shipping 설정 rebuild | 같은 조건의 무편집 warm build |
| --- | ---: | ---: |
| Cargo 완료 로그 | `2m 26s` | `0.11s` |
| timing HTML 전체 시간 | `146.4s` | `0.0s` |
| Fresh / Dirty / Total units | 0 / 62 / 62 | 62 / 0 / 62 |
| jobs / ncpu | 9 / 9 | 9 / 9 |
| timing 관측 max concurrency | 10 | 0 |
| manager receipt 실행 시간 | `146.569s` | `0.251s` |
| manager receipt ID | `1795425504244288723-3` | `1795425719232683541-3` |

warm HTML의 `0.0s`는 모든 unit이 Fresh여서 기록된 unit duration이 0인 결과다.
실제 Cargo 호출 전체가 0초였다는 의미가 아니며 완료 로그의 `0.11s`를 사용한다.
manager 시간은 프로세스 실행·감시 경계가 포함된 별도 측정이므로 Cargo 시간과
섞지 않는다. rebuild의 manager 표본 process-group RSS 최대는 2,269,499,392 B였고
kernel peak는 확보되지 않았다. timing max concurrency 10도 jobs를 10으로 바꿨다는
뜻이 아니다.

### 실제 긴 구간과 dependency 재사용

아래 시각은 rebuild HTML의 `UNIT_DATA`에 기록된 상대 시각이다. section 값은
각 unit 시작점 기준이며 표의 frontend/codegen은 해당 section의 길이다.

| Unit | 시작 | 끝 | Duration | Frontend / codegen |
| --- | ---: | ---: | ---: | ---: |
| `clearra-coverage` | `1.67s` | `60.89s` | `59.22s` | `4.08s` / `55.14s` |
| `clearra-core-ffi`, `default` | `9.69s` | `12.86s` | `3.17s` | `1.29s` / `1.88s` |
| `clearra-postprocess` | `10.46s` | `14.87s` | `4.41s` | `1.26s` / `3.15s` |
| `clearra-core-executor` | `11.72s` | `115.22s` | `103.50s` | `23.71s` / `79.79s` |
| `symbolic_family_compare` bin | `115.23s` | `146.44s` | `31.21s` | 분리 기록 없음 |

주된 마지막 critical path는 executor `103.50s` 뒤의 최종 bin `31.21s`다.
postprocess는 frontend/rmeta가 준비되는 `11.72s`에 executor를 시작시키며,
coverage의 긴 codegen도 executor와 겹친다. 따라서 이 unit들의 duration을 단순히
더하면 전체 wall time을 과대평가한다. executor의 codegen 구간이 frontend보다 길다는
관측은 가능하지만, 최종 bin의 `sections`는 `null`이다. 그 `31.21s`를 linker만의
시간이나 ThinLTO만의 시간이라고 분리할 수 없다.

새 FFI unit `8db114db1aac614a`의 실제 feature는 `default`뿐이다. executor unit
`39a2294374ad0f16`과 bin unit `c386f0df923a6998`은 모두
`default, parallel, placement-dependency-pc, symbolic-family`를 사용했다.
62개 timing unit 중 `test-support` feature를 가진 unit은 없고 executor unit도
하나였다. warm log는 FFI, executor, 최종 tools 패키지까지 모두 `Fresh`로 보고한다.
이는 선택한 tools build에서 dev feature 유입을 제거하고 같은 unit을 재사용했다는
증거다. 여러 패키지를 함께 선택한 build나 example/test target까지 재컴파일이
없어진다는 주장은 하지 않는다.

새 `build/cargo/default/release/symbolic_family_compare`의 ELF `.comment`도
`Linker: LLD 22.1.8`을 기록한다. 기존 진단용 binary와 새 shipping binary 모두
LLD를 사용하므로 이번 결과를 linker 교체의 효과로 설명하지 않는다. 이전
`lto=false`, `codegen-units=16`, `incremental=false` 진단용 빌드와는 source,
dependency feature 및 profile이 다르므로 완료 시간의 차이를 속도 개선율이나
회귀율로 환산하지 않는다. warm 재사용은 확인했지만, 소스 일부를 편집했을 때의
incremental 재빌드 비용이나 별도의 dev/test 정책 비교는 측정하지 않았다.

timestamp HTML은 각각
`build/cargo/default/cargo-timings/cargo-timing-20261123T091824297Z-b98240691fcea4a2.html`,
`build/cargo/default/cargo-timings/cargo-timing-20261123T092159350Z-b98240691fcea4a2.html`이다.
source·profile·target dir·feature를 고정한 원시 로그와 함께 보존한다.

### Row-prefix 개선 후보의 세 binary 빌드

row-prefix projection/semijoin과 Union remainder 변경을 포함한 source tree
`eb87b3b8cac7d31d5702adc61245f04548a396f3`에서는 같은 tools 패키지의 `--bins`를
빌드했다. 대상은 `symbolic_family`, `symbolic_family_compare`, `symbolic_ctk4` 세 개다.
앞선 측정의 실행파일 한 개와 target 집합이 다르고 source도 바뀌었으므로, 두 총시간의
차이를 개선율로 계산하지 않는다.

| 항목 | 관측값 |
| --- | ---: |
| Cargo 완료 로그 | `2m 09s` |
| timing HTML 전체 시간 | `129.9s` |
| Fresh / Dirty / Total units | 60 / 4 / 64 |
| jobs / ncpu / 관측 max concurrency | 9 / 9 / 3 |
| manager receipt 실행 시간 | `130.012s` |
| manager 표본 process-group RSS 최대 | 2,213,539,840 B |
| manager receipt ID | `1795426240980932984-3` |

| Unit | 시작 | 끝 | Duration | Frontend / codegen |
| --- | ---: | ---: | ---: | ---: |
| `clearra-core-executor` | `0.10s` | `97.49s` | `97.39s` | `17.92s` / `79.47s` |
| `symbolic_family_compare` bin | `97.49s` | `129.86s` | `32.37s` | 분리 기록 없음 |
| `symbolic_family` bin | `97.49s` | `111.95s` | `14.46s` | 분리 기록 없음 |
| `symbolic_ctk4` bin | `97.49s` | `105.09s` | `7.60s` | 분리 기록 없음 |

실제로 다시 빌드한 unit은 executor와 세 binary다. 나머지 60개는 Fresh이며,
executor가 끝나면 세 binary가 함께 시작한다. 마지막 critical path는 executor 뒤의
`symbolic_family_compare`다. 나머지 두 binary의 시간을 더해 전체 시간을 계산하면
중복 집계가 된다. 세 binary 모두 `sections=null`이므로 개별 duration을 linker만의
시간으로 해석하지 않는다.

실제 `rustc` 인자는 executor의 `opt-level=3`, `linker-plugin-lto`,
`codegen-units=1`과 세 binary의 `opt-level=3`, `lto=thin`, `codegen-units=1`을
유지했다. 네 unit 모두 동일한 release incremental 경로와
`default, parallel, placement-dependency-pc, symbolic-family` feature 집합을 사용했다.
새 최적화 수준이나 worker 제한을 도입한 빌드가 아니다. 이 측정만으로 소스 변경에
따른 incremental 재사용량을 정량화할 수는 없다.

원시 로그는 `symbolic-prefix-shipping-release-20261123.log`, timing HTML은
`build/cargo/default/cargo-timings/cargo-timing-20261123T093041072Z-b98240691fcea4a2.html`이다.
Rust 재추출 결과의 마지막 unit 끝은 test `47.47s`, 첫 shipping `146.44s`,
warm `0.0s`, 세 binary 빌드 `129.86s`로 표와 일치한다. warm의 Cargo 완료 시간은
별도 로그의 `0.11s`다. unit 끝과 HTML의 반올림된 전체 label, manager 시간을 구분한다.

### 미노 종류별 cache 수명을 정리한 최종 후보 빌드

source tree `6bab3217ddcce8484f11685c3a1f96d4600cc809`의 마지막 shipping `--bins`
빌드는 완료한 미노 종류의 compatibility memo를 해제하는 변경을 포함한다.
대상은 앞 후보와 같은 세 binary이지만 소스와 incremental cache 상태가 다르다.
아래 값은 이 빌드의 기록이며, 앞 후보 대비 개선율로 해석하지 않는다.

| 항목 | 관측값 |
| --- | ---: |
| Cargo 완료 로그 | `2m 01s` |
| timing HTML 전체 시간 | `121.2s` |
| 마지막 unit 끝 | `121.15s` |
| Fresh / Dirty / Total units | 60 / 4 / 64 |
| jobs / ncpu / 관측 max concurrency | 9 / 9 / 3 |
| manager receipt 실행 시간 | `121.276s` |
| manager 표본 process-group RSS 최대 | 1,915,346,944 B |
| manager receipt ID | `1795426936928146498-3` |

| Unit | 시작 | 끝 | Duration | Frontend / codegen |
| --- | ---: | ---: | ---: | ---: |
| `clearra-core-executor` | `0.08s` | `87.93s` | `87.85s` | `6.58s` / `81.27s` |
| `symbolic_family_compare` bin | `87.93s` | `121.15s` | `33.22s` | 분리 기록 없음 |
| `symbolic_family` bin | `87.93s` | `101.57s` | `13.64s` | 분리 기록 없음 |
| `symbolic_ctk4` bin | `87.93s` | `96.58s` | `8.65s` | 분리 기록 없음 |

이번에도 executor 한 unit과 세 binary만 다시 빌드했다. 세 binary는 executor 완료
직후 함께 시작했고, 마지막 critical path는 `symbolic_family_compare`였다. executor의
frontend가 `6.58s`로 기록됐어도 코드 변경·cache 재사용·시스템 상태의 영향을 분리한
실험은 아니므로 이를 특정 변경 하나의 효과로 단정하지 않는다. codegen은 `81.27s`였고
세 binary의 section 분리 기록은 없었다.

실제 로그의 `opt-level=3`, `linker-plugin-lto`/`lto=thin`, `codegen-units=1`,
`strip=symbols`, release incremental 경로를 확인했다. 네 재빌드 unit의 feature는
모두 `default, parallel, placement-dependency-pc, symbolic-family`다.
shipping 최적화나 worker 정책을 낮추지 않았다.

Rust 추출 결과는 `_local/artifacts/cargo-timings-piece-cache-20261123.json`,
원시 로그는 `symbolic-piece-cache-shipping-release-20261123.log`, HTML은
`build/cargo/default/cargo-timings/cargo-timing-20261123T094216970Z-b98240691fcea4a2.html`이다.

## 6. SIGKILL 관측과 원인 판정 한계

아래 값은 계측 하네스와 manager receipt의 관측이다. 세부 입력 및 모든 지문은
기존 [Full4 비교 보고서](full4-symbolic-family-comparison-2026-11-23.md)에 남아 있다.

| 항목 | 기존 경로 | symbolic 경로 |
| --- | ---: | ---: |
| receipt ID | `1795421497885518335-3` | `1795421548085108837-3` |
| manager 실행 시간 | `18.730s` | `128.480s` |
| 종료 | Unix signal 9 / exit 137 | Unix signal 9 / exit 137 |
| manager 2초 표본 process-group RSS 최대 | 3,020,353,536 B | 2,159,460,352 B |
| 자식 1초 표본 VmHWM 최대 | 2,992,443,392 B | 2,787,586,048 B |
| 준비 완료 경계, search 기준 | `0.402721210s` | `99.951986695s` |
| 마지막 세부 문맥 | 준비 marker 뒤, 세부 단계 미관측 | rank 2 / S / row 263 of 764 / `row_down` |
| 완료 결과·전체 검산 | 없음 | 없음 |

두 receipt 모두 timeout 0, `supervisor_termination_reason=null`, pressure events 0,
`automatic_retry=false`, `cancel_on_stdin_eof=false`다. 종료 뒤 남은 descendants는 0이고
tree 정리가 확인됐다. 이는 manager의 시간 제한이나 기록된 메모리 압력 규칙이 종료를
요청했다는 증거와 맞지 않는다. 반대로 manager의 pressure events 0이 커널 OOM이나
외부 watchdog의 부재를 증명하지도 않는다.

symbolic의 마지막 네 1초 표본에서 RSS는 약 1.709→2.788 GB로 증가했으며 phase와
work pulse는 같았다. 큰 reserve/rehash/copy 등 pulse 사이의 저장소 확장과 부합하지만,
어느 allocation이 원인이었는지는 관측하지 못했다. 그 phase는 `row_down`이며
이동 fixed-point나 최종 compaction이었다고 바꿔 설명하지 않는다.

현재 도구 namespace에서는 dmesg와 자기 자식만 대상으로 한 ptrace probe가 EPERM으로
거절됐고, kernel journal에는 항목이 없었다. cgroup OOM counter와 host audit/trace
정보도 확보하지 못했다. `perf`, `bpftrace`, `ausearch`, `auditctl`은 PATH에 없었다.
현재 `/proc/self/limits`의 CPU/RSS/address-space 항목이 unlimited인 사실은 이전
benchmark 자식이나 호스트의 자원 계약이 무제한이었다는 증거가 아니다.

따라서 메모리 압력을 우선 조사하되 발신자를 단정하지 않는다. manager의 정상 Drop
정리는 root child를 wait/reap한 뒤이므로 이미 관측한 원래 자식의 죽음을 설명하지
않는다. 이전 코드에서 정리 시도 자체의 기록이 충분하지 않았다는 점은 아래처럼
보강한다. 이 원인 조사 동안 Full4를 다시 실행하거나 조건을 바꾸지 않았다.

### 종료 요청의 가시성 보강

[`tools/clearra-manage/src/runtime.rs`](../../tools/clearra-manage/src/runtime.rs)에
manager가 직접 보낸 종료 요청의 단계, 대상, signal, 실제 errno/결과 및 wait 관측을
구조화된 stderr와 receipt에 기록하는 경로를 추가했다. receipt에는 Drop 이전 snapshot이
들어가며, 그 이후 Drop 정리는 stderr로 남는다. 이는 manager가 한 일과 외부 SIGKILL을
구별하기 위한 증거다. 외부 발신자를 관측하지 못했는데 kernel OOM이라고 확정하는
대체 증거가 아니다.

빌드 중 별도 도구 관측에서는 workspace filesystem이 overlay였고 build cache의
파일 사용량은 약 7.5 GiB였다. `/home/agent`의 tmpfs 사용량은 857,216 KiB,
`MemTotal`은 10,206,468 KiB, `Shmem`은 5,232,108 KiB였다. 이 관측의 별도 원시
artifact는 보존하지 않았다. 따라서 build target 자체를 tmpfs라고 단정하거나 cache
파일 크기를 그만큼의 상주 RAM으로 환산하지 않는다. namespace에 보이는 PID만으로
shared-memory 사용의 다른 소유자를 특정하거나 그것을 이번 SIGKILL의 직접 원인으로
지목할 근거도 없다.

root가 실행한 termination-audit fixture 1개와 permission-metadata fixture 1개는
각각 통과했다. 기존 실패 실행의 receipt를 새 계측 결과로 소급해 변경하지 않는다.

### Compatibility filter 적용 후의 별도 Full4 관측

새 shipping binary의 Full4는 `SOURCE_TREE=81faa07b0d8cb140813ecc1c12782571e6d40cda`,
binary SHA-256 `2742a04a5adcc7d90dff474f3fa4cf247a09a17ea1042fd1abdca96cefadbfd3`로
식별된다. 입력은 빈 4L, P7P4에 대응하는 Standard7Bag11, Jstris180, empty Hold 허용,
10 locks/11 draws, FullQueueOracle, CountUnique였다. worker policy는 Auto/all-logical,
관측 hardware/affinity는 9였고, 시간·node·candidate 제한은 0이었다. 이 실행은
archive-only 경로를 선택했으며 search 자체가 끝나지 않아 archive나 전체 검산 결과를
만들지 못했다.

| 항목 | 관측값 |
| --- | ---: |
| receipt ID | `1795425735355417354-3` |
| manager 실행 시간 | `34.771s` |
| 종료 | Unix signal 9 / exit 137 |
| manager 표본 process-group RSS 최대 | 2,796,679,168 B |
| 자식 1초 표본 RSS / VmHWM 최대 | 2,768,936,960 B / 2,768,936,960 B |
| geometry compiler 완료, search 기준 | `1.082228605s` |
| compatibility 인증에 관측된 시간 | `0.002094247s` |
| geometry와 인증 완료, search 기준 | `1.084322852s` |
| rank 1 완료가 처음 관측된 표본 시각 | `2.00298158s` |
| 마지막 표본 시각 / 문맥 | `34.023342866s` / rank 2, I, row 21 of 764, `row_down` |
| compatibility memo entries | 52,402,493 |
| compatibility retained bytes 추정 | 763,369,440 B |
| joint ZDD nodes | 9,426,301 |
| joint ZDD retained bytes 추정 | 756,580,736 B |
| filter가 새로 만든 geometry nodes | 0 |
| search / identity export / global coverage / joint comparison 완료 | 모두 false |

메모리 및 work 항목은 각각 독립적으로 읽은 relaxed 진단 필드이므로 한 명령 경계의
원자적인 snapshot은 아니다. retained 추정치는 allocator·임시 rehash/copy·전체
프로세스 메모리의 peak와 다르며, 둘의 합을 RSS와 같은 값으로 취급하지 않는다.
마지막 두 표본에서는 memo/node/work 값이 같지만 RSS가 2,505,932,800 B에서
2,768,936,960 B로 증가했다. 확장·복사 같은 계측 pulse 사이 비용과 맞는 관측이지만,
실패한 allocation의 위치나 SIGKILL 발신자를 확인한 것은 아니다.

이제는 manager가 먼저 자식을 종료했는지를 실제 audit와 대조할 수 있다.
receipt의 `termination_audit`는 `snapshot_phase=before-containment-drop`,
`attempts=[]`, `unretained_attempts=0`이다. `supervisor_termination_reason=null`,
pressure events 0, timeout 0, `automatic_retry=false`도 함께 기록됐다.
stderr에 있는 유일한 종료 시도는 약 `34.7736s`의 `containment-drop`이며
`child_wait_status_observed=true`다. 이 process-group SIGKILL은 반환값 -1,
`os_error_code=3`(ESRCH), `succeeded=false`로 끝났다. 이미 wait로 원래 자식의
죽음을 관측한 뒤의 실패한 정리 시도이므로 앞선 signal 9 종료의 원인이 될 수 없다.

새 receipt는 실제 direct child의 CPU/RSS/address-space rlimit이 unlimited였다는
정보도 남긴다. 그러나 상위 cgroup의 finite limit과 OOM kill counter는 여전히
확보되지 않았다. 이 기록으로 manager의 명시 종료 경로와 구별할 수 있는 범위는
넓어졌지만, 커널 OOM과 외부 발신자 사이의 구분은 남는다. `34.771s`를 완료 검색의
속도라고 비교하거나 이전 `128.480s` 대비 성능 개선율을 계산하지 않는다.

### Row-prefix projection/semijoin 후보의 별도 Full4 관측

다음 실행은 source tree `eb87b3b8cac7d31d5702adc61245f04548a396f3`, binary SHA-256
`27251af6cc3a03643ec0085fd738c3852e658c99242710cecd664dcfa475635f`를 사용했다.
앞 절의 세 binary shipping 빌드 중 `symbolic_family_compare`다. 같은 Full4 입력에서
row-prefix projection/semijoin과 Union remainder를 적용했으며, 시간·worker·메모리
조건을 낮춘 동일 코드 재시도가 아니다.

| 항목 | 관측값 |
| --- | ---: |
| receipt ID | `1795426382130762307-3` |
| manager 실행 시간 | `34.755s` |
| 종료 | Unix signal 9 / exit 137 |
| manager 표본 process-group RSS 최대 | 2,905,223,168 B |
| 자식 1초 표본 RSS / VmHWM 최대 | 2,877,046,784 B / 2,877,046,784 B |
| geometry compiler 완료, search 기준 | `0.893706301s` |
| compatibility 인증에 관측된 시간 | `0.002189804s` |
| geometry와 인증 완료, search 기준 | `0.895896105s` |
| rank 1 완료가 처음 관측된 표본 시각 | `1.003648129s` |
| 마지막 표본 시각 / 문맥 | `34.033166792s` / rank 2, J, row 577 of 764, `row_down` |
| compatibility memo entries | 65,806,082 |
| compatibility retained bytes 추정 | 858,789,856 B |
| joint ZDD nodes | 2,035,494 |
| joint ZDD retained bytes 추정 | 193,462,656 B |
| filter가 새로 만든 geometry nodes | 0 |
| search / identity export / global coverage / joint comparison 완료 | 모두 false |

rank 2에서 관측한 큐 관계의 저장소와 memo 규모를 구분해 기록한다. 이전 후보와
마지막 piece/row 문맥이 다르며 둘 다 rank 2를 완료하지 못했다. 따라서 ZDD node 수,
RSS 또는 종료 시간의 차이를 같은 작업량에서의 개선·회귀율로 계산하지 않는다.
많은 compatibility memo가 남은 상태에서 종료됐다는 사실은 확인되지만, retained
추정치는 임시 allocation이나 allocator까지 합한 RSS 상한이 아니다.

이 실행의 audit 역시 `attempts=[]`, `unretained_attempts=0`,
`snapshot_phase=before-containment-drop`이다. timeout 0, pressure events 0,
`supervisor_termination_reason=null`, `automatic_retry=false`였다. 유일한 stderr
종료 시도는 약 `34.7581s`, `child_wait_status_observed=true`인 Drop 정리였고,
SIGKILL 요청은 ESRCH(errno 3)로 실패했다. 이 시도도 이미 관측한 자식 종료의 원인이
아니다. OOM counter와 외부 발신자 증거가 없는 한 SIGKILL의 최종 원인은 단정하지 않는다.

### 미노 종류별 cache 수명을 정리한 최종 Full4 관측

최종 실행은 source tree `6bab3217ddcce8484f11685c3a1f96d4600cc809`, binary SHA-256
`965ee9045c36b5566e80929ccf070ed8658443598da5ec24c86469f5577c9aba`를 사용했다.
완료한 미노 종류의 compatibility memo를 해제하는 변경을 적용했고, 입력 및 shipping
profile·worker·시간 제한 조건을 유지했다. 이 실행에서는 rank 2 완료 marker가
관측됐으나 전체 search는 완료되지 않았다.

| 항목 | 관측값 |
| --- | ---: |
| receipt ID | `1795427063731448912-3` |
| manager 실행 시간 | `44.694s` |
| 종료 | Unix signal 9 / exit 137 |
| manager 표본 process-group RSS 최대 | 2,851,733,504 B |
| 자식 1초 표본 RSS / VmHWM 최대 | 2,708,467,712 B / 2,725,687,296 B |
| geometry compiler 완료, search 기준 | `0.928935797s` |
| compatibility 인증에 관측된 시간 | `0.002136382s` |
| geometry와 인증 완료, search 기준 | `0.931072179s` |
| rank 1 완료가 처음 관측된 표본 시각 | `2.002679851s` |
| rank 2 완료가 처음 관측된 표본 시각 | `37.045454304s` |
| 마지막 표본 시각 / 문맥 | `44.05885235s` / rank 3, `movement` |
| 마지막 compatibility memo entries | 0 |
| 마지막 compatibility retained bytes 추정 | 6,112 B |
| 마지막 joint ZDD nodes | 786,432 |
| 마지막 joint ZDD retained bytes 추정 | 55,586,048 B |
| 마지막 Boolean 누적 work counter | 224,898,786 |
| search / identity export / global coverage / joint comparison 완료 | 모두 false |

마지막 표본에서는 compatibility cache가 비워진 상태이며, rank 3 이동 관계 계산의
Boolean 누적 work가 증가하고 있었다. 이 work 값은 Boolean node 수나 개별 rank만의
비용이 아니다. 마지막 ZDD/cache retained 값은 이전 단계의 최대치나 전체 프로세스의
peak RSS와도 다르다. 따라서 cache의 6,112 B와 ZDD의 55,586,048 B를 근거로 실행
전체의 메모리가 그 수준으로 줄었다고 해석하지 않는다.

종료 문맥이 앞 후보의 rank 2 `row_down`에서 rank 3 `movement`로 달라졌다는 점은
관측 사실이다. 완료하지 못한 서로 다른 작업 범위의 `34.771s`, `34.755s`, `44.694s`나
각 RSS를 같은 작업량의 성능 비교로 사용하지 않는다. rank 2 완료도 전체 해법 집합의
정확성 검산, accepted-joint archive 생성 또는 Full6 실행 가능성을 대신하지 않는다.
Full4 완료·전체 비교가 없으므로 이 작업의 Full6 진행 판단은 false로 유지한다.

최종 audit도 Drop 이전 `attempts=[]`, `unretained_attempts=0`이며 timeout 0,
pressure events 0, `supervisor_termination_reason=null`, `automatic_retry=false`다.
stderr의 유일한 SIGKILL 요청은 약 `44.6965s`에 발생한 post-wait Drop 정리였고
`child_wait_status_observed=true`, ESRCH(errno 3), `succeeded=false`였다. 이 실패한
정리 요청은 앞서 관측한 자식 종료의 원인이 아니다. 외부 발신자 및 kernel OOM에
대한 증거 범위는 이전 실행과 같으며, 최종 원인을 확정하지 않는다.

## 7. 구조 개선의 정확성 및 메모리 검토

새 [`GeometryCompatibilityFilter`](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_compatibility_filter.rs)는
완성 cover를 열거하거나 새 derivative expression을 만들지 않고 원본 family를 직접
적용한다. 모든 cover가 동일한 셀 집합 R을 겹침 없이 채우고, 선택 row가 모두
nonempty이며 R 안에 있을 때, cover에 없는 선택 row는 cover의 어떤 다른 row와
반드시 겹친다. 각 cover row와 충돌하는 다른 선택 row를 제거하는 guard의 논리합은
따라서 정확히 Down(F) membership을 계산한다.

Product는 같은 joint member에 양쪽 filter를 순차 적용하므로 큐와 row의 상관관계를
독립적인 곱으로 바꾸지 않는다. 생성자는 Append의 비겹침, Union의 같은 영역,
Product의 분리된 영역 및 root의 R 일치를 검사한다. ZDD compaction 이후에는 memo를
무효화하며 rank 경계에서는 copy 전에 memo를 해제한다. 최종 후보는 완료한 미노
종류의 작업 경계에서도 compatibility memo를 해제한다.

이 구조가 모든 큰 입력에서 작은 메모리를 보장하지는 않는다. 인증은 DAG 전체에 대해
임시 `u64 regions` 배열, 약 `8 × (node_count + 2)` bytes를 사용한다. 이후에도
`(geometry node, joint root)`와 `(row, joint root)` memo는 한 rank 안에서 커질 수 있다.
retained-byte 지표는 이 인증용 임시 배열이나 allocator 전체의 peak RSS를 포함하지
않는다. 첫 두 개선 후보의 Full4는 geometry와 인증 이후 rank 2의 compatibility 관계
memo와 ZDD가 커지는 문맥에서 종료됐다. 최종 후보는 해당 rank를 완료하고 rank 3의
이동 관계 계산 중 종료됐다. 새 derivative geometry node를 만들지 않는 것만으로
전체 메모리 문제를 해결한 것은 아니며, 종료 문맥의 변화가 관측됐다. 이후 구조 수정은
별도의 source revision과 실행 증거로 평가해야 한다. 완료한 Full4/Full6의 속도와 peak
개선율은 아직 없다.

기능 검증은 algebra 3개와 기존 solver 대비 finite fixture 4개의 통과 범위다.
완성된 Full4 전체 비교나 이미 알려진 확률 결과의 추가 증명으로 확대하지 않는다.

## 8. CTK4의 cold open과 lazy 조회 계약

[`clearra-ctk4`](../../crates/clearra-ctk4/README.md)는 ordered ZDD의 공유 node table과
named roots를 저장하는 별도 codec이다. 원자와 metadata는 opaque이며, codec 검증이
게임 합법성·검색 완결성·원본 source 의미를 자동으로 인증하지 않는다.

| 경로 | 읽는 범위와 부여되는 권한 |
| --- | --- |
| `UnverifiedArchive::open` | header/manifest만 읽음; count는 선언값이며 exact count/contains/page API 없음 |
| 알 수 없는 파일의 `verify_all` | 모든 atom block과 node를 확인하고 참조·변수 순서·checked count recurrence·root count를 검사 |
| `write_archive` | 실제 직렬화 후 같은 전체 scanner를 통과해야 `ProducerSeal` 반환 |
| 살아 있는 seal로 reopen | 동일 manifest digest와 길이를 확인한 뒤 lazy exact 조회 허용; 읽는 block마다 checksum 검사 |

unknown file의 exact cold open은 전체 graph 검증이 필요하다. graph node 수 N에 비례한
논리적 검증 방문이 있으며, bounded cache에 비해 넓은 graph는 child block을 다시 읽고
decode할 수 있다. 모든 byte를 정확히 한 번만 읽는 I/O 상한이나 임의 unknown 파일을
sublinear 비용으로 검증한다는 주장은 하지 않는다. atoms와 manifest를 읽는 비용도
별도 존재한다.

`ProducerSeal`에는 공개 생성자나 deserializer가 없다. 문서에 적힌 checksum만으로
이 권한을 만들 수 없으며, 영구 서명도 아니다. 다른 프로세스에서 다시 여는 파일은
별도의 인증된 애플리케이션 권한 경계가 없다면 전체 검증을 해야 한다. seal을 사용한
경로만 node/atom block을 가져오지 않고 열어 두었다가 요청한 page 경로를 읽을 수 있다.

page는 archive 변수 순서의 low-before-high 순서이며 CTK1 lexical 순서가 아니다.
독립 decoder는 이 순서와 manifest/root/rank identity를 유지해야 한다. source adapter의
해석과 CTK3 표시용 변환은 별도 경계다. 현재 codec의 generic count는 atom-ID set의
count이며 외부 dictionary의 injectivity나 세 root의 projection 관계까지 인증하지 않는다.

root가 실행한 codec 15개 fixture는 통과했다. 이 결과는 큰 실파일의 cold-open 시간,
압축률, browser/network async adapter 출하, 또는 완료된 Full4 결과의 저장을 의미하지
않는다. 이 조사에서 Full4의 accepted joint relation 파일은 생성되지 않았다.

## 9. 로컬 증거 위치

원시 로그는 `_local/artifacts/`에 있고 연구 문서에는 검토한 요약만 포함한다.

- `full4-symbolic-release-library-20261123.log`
- `full4-symbolic-comparison-build-20261123.log`
- `full4-symbolic-comparison-build-fixed-20261123.log`
- `full4-symbolic-membership-tests-20261123.log`
- `compatibility-algebra-tests-20261123.log`
- `compatibility-algebra-tests-fixed-20261123.log`
- `compatibility-solver-equivalence-20261123.log`
- `build-timings-manager-20261123.log` — manager dev build `2.08s`; core build 시간과 별도
- `runtime-termination-audit-tests-20261123.log`
- `runtime-permission-metadata-tests-20261123.log`
- `ctk4-codec-tests-20261123.log`
- `symbolic-shipping-release-timings-20261123.log`
- `symbolic-shipping-release-warm-timings-20261123.log`
- `symbolic-prefix-shipping-release-20261123.log`
- `symbolic-piece-cache-shipping-release-20261123.log`
- `cargo-timings-extract-20261123.rs`
- `cargo-timings-reviewed-20261123.json`
- `cargo-timings-piece-cache-20261123.json`
- `full4-symbolic-compatibility-20261123.log`
- `full4-symbolic-compatibility-20261123.runtime.json`
- `full4-symbolic-compatibility-20261123.observation.json`
- `full4-symbolic-prefix-20261123.log`
- `full4-symbolic-prefix-20261123.runtime.json`
- `full4-symbolic-prefix-20261123.observation.json`
- `full4-symbolic-piece-cache-20261123.log`
- `full4-symbolic-piece-cache-20261123.runtime.json`
- `full4-symbolic-piece-cache-20261123.observation.json`

첫 core 및 shipping rebuild/warm timing HTML과 release/debug fingerprint JSON은 해당 Cargo cache에 있다.
이 표의 원시 자료는 CI 결과가 아니며 새로운 GitHub Actions 실행도 만들지 않았다.

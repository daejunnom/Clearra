# v0.8.1 exact accelerator benchmark harness

이 문서는 **벤치마크 코드 준비 상태**를 설명한다. 측정값이나 v0.8.1 성능 Go를
기록하지 않는다. 실행기 추가만으로 legal-board나 조건부 도달성 pack의 실제 속도
이득을 주장하지 않는다.

## 권위와 범위

- `scripts/benchmark/fixtures/v081-accelerator-cases.json`은 다섯 kick profile의
  빈 필드 4L P7P4, 초기 필드가 있는 P7, 작은 1L fixed queue 및 Build cover를
  분리한다. 기존 전체 집합 차분에서 확인한 다섯 profile의 개수·집합 해시는
  탐색의 조기 종료 기준이 아니라 완료 후 oracle이다.
- `scripts/benchmark/run-v081-accelerator-abba.mjs`는 **기존** native CLI 바이너리와
  **이미 설치된** 두 signed asset을 사용한다. Cargo/WASM/Cloud Build, 다운로드,
  자산 생성, TB 조회, 자산 설치를 실행하지 않는다.
- 한 실행은 한 case, 한 worker 수, 한 비교쌍만 측정한다. 비교쌍은 baseline
  (두 자산 off) 대 legal-board 단독, 조건부 도달성 단독, 두 자산 결합이다.
  `A-B-B-A`와 역순 라운드를 교차한다. 동일 바이너리와 fixture를 유지한다.
- 명시 `--workers`는 1·2·11 등 요청값을 바꾸지 않는다. 표시 가능한 논리
  프로세서보다 큰 값은 명시적 oversubscription 허용 없이는 거절한다.
- 완료 표본은 실제 `summary.workers_used`가 요청값과 정확히 같아야 한다.
  다중 워커 표본은 `cpu_parallel_execution=true`도 요구한다. 불일치는 시간값이
  빨라도 `invalid`로 기록하므로 직렬 표본이 병렬 ABBA로 집계되지 않는다.
- 모든 fixture에서 `--max-candidates`를 제거하고 하네스 옵션으로도 거절한다. 현재 CPU executor는
  명시 자원 한도가 있으면 serial accounting 경로를 선택하므로, 감독 실행의
  timeout·메모리 제한은 유지하면서 solver에 명시 후보 상한을 전달하지 않는다.
- 실행 전후 `legal-board status`와 `reachability-pack status`로 해당 profile의
  signed generation 설치·자격과 세대 변화 여부를 검사한다. untimed 1피스
  preflight로 실제 `wasm-cpu` 경로도 확인하여 native-core-only 바이너리가 두
  옵션을 무시한 채 A/B를 통과하지 못하게 한다. 실행 receipt에는 바이너리와 fixture
  SHA-256, 두 자산의 catalog/generation, 요청 worker와 실제 CLI runtime
  identity가 남는다. 원본 자산이나 비밀 값은 receipt에 복사하지 않는다.
- receipt v2는 각 solver child의 Windows process-tree working-set peak를 250ms
  간격으로 기록한다. PowerShell/CIM sampler의 관찰 비용은 두 arm에 대칭 적용되고
  timed wall 값에 포함된다. 이는 sampled working set이지 순간 peak RSS나 공유
  accelerator owner의 정확한 메모리 증명은 아니다. Windows 이외는 명시적으로
  unsupported로 표기한다. 서로 다른 memory probe 계약의 receipt는 합칠 수 없다.
- 각 샘플의 성공은 완결된 JSON summary와 지정된 candidate/result/coverage
  필드로 대조한다. 큰 집합의 첫 페이지에만 해법 키가 있더라도 완전한 개수·집합
  해시를 제공하면 유효하며, 그 페이지를 전체 해법으로 오인하지 않는다. Build
  cover는 중첩된 완전성 증거까지 검사한다. 독립 결과가 있는 경우 oracle도 확인한다.
  timeout 및 typed resource-limit는 완료 시간이나 UNSAT가 아닌 censored다.
  출력 한도, 다른 오류, 의미 불일치는 invalid로 중단한다.
- `summarize-v081-accelerator-abba.mjs`는 같은 case, worker, 실행 파일, fixture,
  자산 세대, 결과 의미인 짧은 배치만 합친다. 각 arm에서 완결·무 censor 표본
  20개 이상일 때만 p95 전체 지연 판정을 계산한다. 큰/Build 입력의 임시
  문턱은 10% 개선, 작은 입력은 5% 이하 회귀다.
- `compare-v081-accelerator-workers.mjs`는 worker 수별 시간을 평균내지 않고
  1·2·11 worker의 동일 실행 파일·asset generation·결과 identity만 검사한다.
- Core의 병렬·직렬 집계는 legal-board prune 외에 조건부 도달성의 요청 여부,
  정책 활성 여부, in-process snapshot 활성 여부, 실제 lookup 수, 빈 entry set,
  cache short-circuit, OutOfScope, Unknown, SnapshotMismatch, InvalidAsset 및 complete hit/miss를
  구분해 기록한다. qualified asset의 설치 상태만으로 요청에서 활성화됐다고
  간주하지 않는다. 조건부 treatment가 0 hit면 속도 표본으로 인정하지 않고 이
  진단 필드로 정책 gate·snapshot·query domain·asset 상태를 먼저 분류한다.

## 추후 실행 계약

벤치마크용 별도 빌드는 이 단계에서 준비하거나 실행하지 않는다. 실행 시에는
이미 존재하는 `clearra` 바이너리의 절대 경로, 두 제품의 설치 루트, 새 run ID를
명시한다. 실행 프로세스 전체는 저장소의 `benchmark-search` 감독 프로필로
감싼다. 출력은 선언된 `_local/artifacts/v081-accelerator-abba/<run-id>`에만
남는다. 120분 감독 한도보다 최악 시간이 길면 라운드를 분리한다.

```text
clearra-manage runtime run --producer benchmark --profile benchmark-search \
  --timeout <seconds> -- node scripts/benchmark/run-v081-accelerator-abba.mjs \
  --binary <existing-clearra-absolute-path> \
  --legal-root <installed-legal-board-base-absolute-path> \
  --conditioned-root <installed-reachability-base-absolute-path> \
  --case pc-p7p4-srs-plus --pair legal --workers 11 \
  --rounds 2 --timeout-ms 300000 --run-id <new-run-id>
```

이는 실행 예시일 뿐 이 단계에서 실행한 명령이 아니다. case/pair/worker가
다른 실험은 별도 run ID를 사용한다. 반복 배치의 `summary.json`만 읽어 합친다.

```text
node scripts/benchmark/summarize-v081-accelerator-abba.mjs \
  <first-summary.json> <second-summary.json>
```

## 아직 속도 권위로 삼지 않는 항목

- 이 실행기는 새 native 프로세스 시작부터 JSON 출력 후 프로세스 종료까지의
  제품 지연을 잰다.
  CLI가 제공하지 않는 `CandidateProjection`, 실제 BuildUp,
  `BuildOrderReachability`, reducer, render 시간은 임의로 추정하지 않는다.
  stage-profiling을 켠 별도 관찰과 일반 사용자 지연 실행을 혼합하지 않는다.
- `resource_peak_cpu_bytes`는 솔버의 논리적 보고값이다. sampled process-tree
  working set과 실제 공유 asset peak는 서로 다른 값이며, 128MiB gate는 별도의
  owner 계측까지 Open이다.
- 집합 해시와 첫 canonical candidate ID는 전체 출력 순서의 증명이 아니다.
  전체 canonical order와 CLI/GUI/Discord 결과 parity는 별도 검증이 필요하다.
- native CLI 수치만으로 WASM GUI의 lazy OPFS 설치, 분산 verifier의 owner
  공유, Discord 이미지 readback을 대신하지 않는다.
- 각 비교쌍의 p95를 구해도 개별 이득과 결합 이득, 작은 입력 회귀,
  BuildUp p95 20%, 제품 p95 10%를 모두 충족하기 전에는 v0.8.1 Go가 아니다.

코드 수준 검증은 `node --test scripts/benchmark/v081-accelerator-abba-core.test.mjs`로
수행한다. 이것은 solver benchmark나 빌드가 아니다.

## 2026-09-27 워커 메모리 소유권 계측

`run-worker-memo-abba.mjs`의 v2 영수증은 워커 종료 시 private retained payload의
합계와 요청 소유 불변 StandardBag cursor/suffix 테이블을 분리한다. 후자는 워커 수를
곱하지 않고 한 번만 기록하며, memo payload는 private StandardBag 안의 부분량이므로
다시 더하지 않는다. 합계 불일치, 누락된 shared 항목과 이전 혼합 계측 scope는 표본을
거절한다. 이 논리량과 감독 실행기의 aggregate commit peak는 서로 대체하지 않는다.
또한 두 값 모두 legal-board/조건부 pack의 active shared-owner 128MiB 증명과는 별개다.

`v081-worker-memory-accounting.test.mjs`는 위 분리와 중복 합산 거절을 검사한다.
실제 시간·메모리 채택 판정은 같은 바이너리 ABBA 이후에만 가능하며, compact memo의
기존 reference 기본값은 새 자료 없이 바꾸지 않는다.

가속기 ABBA에도 같은 논리 계측을 추가했다. native parallel exit snapshot이 없는
경로는 zero로 만들지 않고 `no-native-parallel-worker-exit-snapshot`으로 남긴다.
shared/private 계약이 없는 과거 영수증은 새 계약 영수증과 합치지 않는다. 새 legal
prune count는 도달성 전에 거부한 child subset의 최초 횟수이며 과거 dead-node count와
직접 비교하지 않는다. conditioned complete hit는 완료된 Boolean 쿼리를 포함하며
전체 lock-family를 증명한 횟수로 해석하지 않는다.

## 2026-09-27 후속 product memo 후보

`run-worker-memo-abba.mjs --candidate compact|state-major`는 baseline을
`CLEARRA_STANDARD_BAG_MEMO=reference`와 product layout `flat`으로 고정한다.
compact treatment는 기존 compact/flat이고, state-major treatment는 reference union과
state-major product만 바꾼다. 두 환경변수를 명시해 caller의 local A/B 설정이
기준군을 오염시키지 않도록 한다. 실제 solver의 backend/layout/storage echo를 작은
비계측 standard-bag query로 먼저 확인하므로 이전 binary의 옵션 무시는 큰 P7P4
실행 이전에 거절한다. selector 자체는 local-search-ab 전용이며 제품 기본값을 바꾸지 않는다.

새 memo 영수증은 v3이며 candidate/양 arm의 정확한 선택을 기록한다. 이 전용
메모리 하네스는 native parallel exit 계측이 없는 1-worker serial을 실행 전에 거절한다.
serial 자료를 private/shared zero로 만들지 않으며 serial correctness/time은 별도
일반 가속기 하네스에서 검증할 수 있다. supervisor memory pressure 표본은 완료된
정확성 증거로 남겨도 clean 성능 채택 수에 넣지 않는다.

가속기 하네스는 reference/flat을 명시 고정한다. 서로 다른 memo selection 또는
selection 계약이 없는 이전 영수증과의 혼합 집계는 거절한다. 2026-09-27 기존
8회 ABBA는 이후 소스 변경의 벤치마크로 재사용하지 않는다.

state-major의 실제 summary에서 product storage와 기존 memo storage는
`state-major`, union storage는 `reference`다. 환경변수의 reference는 flat/union의
대조군을 선택하므로 product의 실제 storage label과 혼동하지 않는다. 사전 확인은
기존 필드 `0x3c0f03c0f`, 4L, 6개 배치, P7, 2-worker의 비계측 요청이다.

하네스는 product/union entry payload 합이 기존 nested memo payload와 같음을
검사한다. 별도 product directory bytes는 private StandardBag 내부에 한 번만
포함되고 private 합계에 다시 더하지 않는다. entry/capacity와 active/allocated/slot
수의 모순도 거절한다. row directory의 빈 슬롯·row prefix를 계측에서 빼지 않는다.
이 자료는 exit-time logical retained accounting이고 allocator overhead·OS peak는
감독 영수증으로 따로 본다.

state-major row는 `(depth, normalized bag, hold)`를 한 번 저장하고 full
`language:u32 → root:u32`를 가진다. row hasher는 기존 full-key SplitMix64 입력을
복원하며, logical recycle trigger는 기존 12B/key 추정 그대로다. entry 표현만
줄였다는 이유로 총 RSS 절감이나 더 빠른 실행을 주장하지 않는다. 이 후보는
local-search-ab에서만 제공하며 새 binary의 별도 ABBA 이전에 기본값으로 채택하지 않는다.

## 2026-09-27 동적 product memo 검증

후속 `adaptive` 정책은 새 요청을 flat으로 시작하고 실제 live entry 비용이 충분할
때만 State-major로 전환한다. 제품의 동적 기본 정책과 달리, 강제 flat/State-major 및
compact 선택 환경변수는 여전히 `local-search-ab` 전용이다. 위 v3의 강제 후보 기록은
역사 증거로 보존하며 이 정책의 측정으로 재명명하지 않는다.

v4 메모 하네스는 다음 고정 입력을 별도 suite로 지원한다.

| `--case` | 입력 | 동적 treatment의 필수 증거 |
| --- | --- | --- |
| `pc-p7p4-srs-plus` (기본값) | 빈 필드 4L, 10개 배치, P7P4 | 실제 promotion이 1회 이상, 456,923개 완결 결과 |
| `pc-existing-field-p7-srs-plus` | 기존 필드 `0x3c0f03c0f`, 4L, 6개 배치, P7 | flat 유지, directory/attempt/promotion 0, 246개 완결 결과 |

`--candidate adaptive`에서 정책 label과 실제 layout을 구분한다. 워커별 크기가 다르면
실제 layout은 `mixed`일 수 있으며 promotion 수와 일관될 때만 인정한다. 작은 입력도
timed sample마다 요청한 워커 수와 실제 CPU parallel 실행을 검사한다. 2-worker tiny
preflight는 별도 저장하지만 측정 평균에 넣지 않는다.

큰 suite는 `--rounds 1`만, 작은 suite는 `--rounds 1|2`를 허용한다. 두 번째 작은
round는 BAAB로 순서를 반전하고 `(round, slot, storage)` 파일명으로 기존 표본을
덮어쓰지 않는다. 각 sample의 timeout은 그대로 보존하며 남은 outer lease가 부족하면
새 sample을 시작하지 않는다. 11-worker P7P4는 1,800초/sample 계약을 유지한다.

영수증에는 solver binary/source와 함께 하네스 파일들의 SHA256을 기록한다. 같은
suite 동안 binary를 교체하지 않는다. preflight와 timed sample 모두 정상 감독 종료와
전체 tree 정지를 요구하며 pressure가 발생한 표본은 clean 성능 채택에서 제외한다.
OS peak는 Windows Job Object의 aggregate commit bytes, private/shared 표는 종료 시
논리 retained bytes다. 둘을 working set 또는 active asset peak로 바꿔 부르지 않는다.
작은 입력의 supervisor-start-through-exit wall time은 시작/종료 비용을 포함하므로
순수 solver 시간이나 p95 회귀 5% gate를 단독으로 증명하지 않는다.

## native 도달성 템플릿 공유 이후의 회계 계약

후속 코드의 private memory scope는 `native-worker-exit-private-retained-payload-sum.v2`다.
불변 StandardBag owner와 새 reachability template/sky-entry owner를 각각 요청당
한 번 기록한다. `shared_reachability_template_retained_bytes` 누락은 0으로 보충하지
않고 거절한다. 실제 같은 Arc가 공유되는 템플릿은 private 항목에서 제외하지만
프로필·크기 mismatch fallback의 private 템플릿은 계속 포함한다.
메모 하네스 schema는 v5이며 가속기 하네스도 같은 v2 회계 계약을 사용한다.
v4 이하 메모 영수증 또는 이전 private scope와의 혼합 집계는 허용하지 않는다.

코드와 순수 계약 테스트만 갱신했으며 새 benchmark는 실행하지 않았다. 기존
ABBA는 그 당시 binary의 자료로 보존한다. 새 논리 shared/private 합계도 OS peak,
모든 Web verifier의 shared ownership 또는 asset peak 128MiB를 증명하지 않는다.

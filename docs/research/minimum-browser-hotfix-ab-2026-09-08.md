# 최소 해법 핫픽스: 알고리즘과 워커 정책의 실제 브라우저 A/B

## 범위와 완료 조건

현재 배포 소스 `3571fd9`와 분리된 `codex/v0.8.0-hotfix-minimum-algorithm-ab`의
후보 연구다. main에 포함하지 않았으며, 외부 Qnia/OR-Tools 구현은 제품에 복사하지
않았다. 첫 canonical 집합 3초 목표는 아직 달성하지 못했고 P2 후속 과제로 남는다.

로컬 Rust→WASM 빌드만 사용했다. native Rust/Cargo 제품 테스트 실행은 하지 않았다.
브라우저에서는 제품 `clearraWorker`와 전체 하위 worker 그래프를 사용했으며,
전용 실행 화면에서 command/progress/terminal을 계측했다. 전체 GUI의 결과 필드
첫 paint 비용을 재는 실험이나 실제 S23 Ultra 실기 성능 측정은 아니다.

- 입력: `ctk3_w0kCQBjwwAMPPAD37g`, P7, 4L, hold, Jstris 180.
- 명령: `clearra pc minimals --lines 4 --board-mask 0x3c0f03c0f --height 4 --pieces 6 --patterns P7 --hold empty --rule jstris-180 --backend cpu --cpu-warmup --workers 6`.
- 기본 구성: reported logical processors=7, requested compute workers=6.
  별도 all-logical 대조는 workers=7이다. 실제 호스트는 Windows/Chrome, 12 logical
  processors이다. 이 설정을 S23 Ultra 에뮬레이션으로 표현하지 않는다.
- warmup/module download는 제외하고 public command부터 first terminal까지 측정했다.
- local fixture deadline은 180초이며 제품 시간 제한이 아니다.
- 실험은 4195만 사용, 서버 lease 45분, 자동 재시작 없음. 4194/8790은 변경하지 않는다.

## 실험 1: M1과 알고리즘 G의 분리

하나의 WASM에서 입력 독립적인 flag 두 비트로 정책만 바꿨다. 실험 setter는
`minimum-hotfix-ab` feature에서만 존재하고 active job 중에는 변경을 거부한다.
일반 제품 빌드에는 setter가 없다. known K=25나 candidate ID는 정책 입력이 아니다.

WASM SHA256: `df6d733daf8b6052c398f79d8179e3ccda8bcaed17c28e01cf7c6af4662acccb`.

| 변형 | 첫 canonical terminal | wave 수 | 판정 |
| --- | --- | --- | --- |
| A: 기존 warm repair + 기존 lower bound | 36,149.445ms | 25 | 기준선 |
| M1: 전역 repair 삭제, 빠른 remote dispatch | 82,015.800ms | 31 | 심각한 회귀, 폐기 |
| G: 연결 성분별 정수 올림 lower bound | 40,224.185ms | 25 | 개선 입증 실패, 기본 비활성 |
| G+M1 | 82,580.890ms | 31 | 채택하지 않음 |

M1의 시작 병렬화 자체는 빨라졌지만 유용한 전역 witness를 잃어 실제 proof 비용이
더 커졌다. 따라서 단순히 serial prepass를 없애는 패치는 승격하지 않는다.
G는 기존 governed scratch를 재사용한 독립 구현이다. 서로소 odd component의
정수 하한은 강화할 수 있어도 이 P7에선 계산 비용을 상쇄하지 못했다.

## 실험 2: M2, 전역 witness를 유지한 중첩 실행

M2는 n-1 구성의 remote workers를 먼저 준비/발행하면서 전역 positive-only repair를
관리 워커에서 한 step씩 진행한다. 매 step 뒤 host에 제어를 돌려 ready/receipt를
처리한다. full-query witness는 새 cube receipt가 아니며, 이미 발행한 task는 실제
receipt를 모두 회수한다. 부정 답·취소·오류를 최소 개수 증명으로 변환하지 않는다.

WASM SHA256: `492470026f54f3c598c2354275d6051eab33d40d7294ce05812307091447957d`.

| 순서/수정 경계 | 변형 | 첫 terminal | 결과 |
| --- | --- | --- | --- |
| 초기 host | A | 40,960.980ms | 성공 |
| 초기 host | M2 | 491.925ms | 실패: 아래 race, 성능 성공 표본 아님 |
| task-source 종료 guard 수정 후 | M2 | 34,937.490ms | 성공 |
| 동일 host | M2 | 35,614.565ms | 성공 |
| 동일 host | A | 40,347.590ms | 성공 |
| 동일 host | A | 74,352.165ms | 성공, 느린 표본도 유지 |

두 M2 표본은 약 35초였으나 표본 수가 작고 A 편차가 크므로 고정된 개선율이나
모든 입력의 개선을 선언하지 않는다. 같은 host 수정 경계의 A 중앙값은 57,349.878ms,
M2 중앙값은 35,276.028ms이지만 한 개의 느린 A가 비율에 크게 작용한다.

M2에서는 profiled wave 바깥 finalize 시간이 약 488~522ms였고 A의 세 표본에서는
약 10,182~10,829ms였다. 이것은 serial barrier가 줄었다는 구간 근거이지 전체
solver 비용이 그만큼 감소했다는 뜻은 아니다. remote proof 자체가 여전히 지배적이다.
M2는 29 waves / remote tasks 560~569개, 관리 워커 advisory compute 합계
324~351ms를 기록했다. 프로파일의 겹치는 clock을 합산하지 않는다.

### 발견해서 고친 경계 오류

첫 M2는 `E_WASM_MINIMUM_PARALLEL_MEMORY: task issuance exceeds control admission`
오류를 냈다. WASM linear memory는 약 3.2MiB였고 실제 메모리 고갈을 입증한 것이 아니다.
전역 repair가 **첫 replica 준비보다 먼저** 유효 witness를 찾으면, 아직 task가
발행되지 않은 query는 닫힌다. host가 이후에도 task descriptor를 요청하면서
없는 query descriptor를 메모리 거부로 표시했다.

- host: core가 validated positive를 알린 뒤에는 추가 task를 요청하지 않는다.
- core/runtime: 발행 전 warm winner는 빈 task source로 처리한다. 메모리 한도를
  늘리거나 guard를 끄지 않는다.
- receipt: advisory witness를 이미 발행한 cube의 `received=true`로 표시하지 않는다.
- 회귀 contract: 모든 remote가 준비 중일 때 warm이 승리하는 순서를 재현한다.

## 최종 후보 및 all-logical 확인

최종 확인용 빌드는 위 runtime 방어와 all-logical/shared controller 경계를 포함한다.
n개 compute slots를 모두 사용하는 경우 관리 워커가 이미 proof shard를 맡으므로
동일 관리 워커에 두 CPU 작업을 겹쳐 넣지 않는다. n-1의 전용 controller 경로만
원격 proof와 advisory repair를 겹친다. 모든 성능 실행은 빌드가 끝난 뒤 진행했다.

최종 WASM SHA256: `7c5aef79bf25a198935b58643ad55f12054aff49714c7e3e672fd6c88145c308`.
manifest의 runtime identity는 `unverified-local-build`이다. 이는 본 source-bound
로컬 비교용 바이너리이며 release acceptance 바이너리로 재사용하지 않는다.

6워커의 동일 binary/host 순서 `A M M A`:

| 변형 | command→first canonical terminal | finalize 진입→첫 remote task 발행 |
| --- | --- | --- |
| A 첫 표본 | 42,404.650ms | 13,694.415ms |
| M2 첫 표본 | 23,359.350ms | 193.145ms |
| M2 두 번째 | 23,758.920ms | 185.260ms |
| A 두 번째 | 24,661.980ms | 4,597.090ms |

전체 시간의 고정 개선율은 확정하지 않는다. A의 반복 편차에 browser tiering/cache,
host 경쟁, 병렬 witness/proof 경로 차이가 영향을 줄 수 있지만 이번 실험은 각각을
독립 계측하지 않았다. **반복해서 재현한 것은 n-1 시작 직렬 barrier의 감소**다.
M2의 first task는 해당 wave의 전체 replica 준비 완료보다 빨리 발행됐다.
두 M2 표본에서 29개 wave 중 각각 27개/28개가 all-ready 이전에 발행했다.
remote workers=6, controller_control_only=true, sampled total active peak=6이다.
관리 워커는 증명 cube를 중복 소유하지 않았고 advisory compute는 합계 242~244ms였다.

7워커는 `--use-all-cpu-threads`를 명시한다. 최초 실험에서 이 옵션을 누락한 입력은
4.890ms에 reserved logical processor 오류로 거부됐다. 이는 정상 정책 검증이며
탐색 성공 시간으로 포함하지 않는다. 일반 GUI의 명시 옵션을 빼고 하드웨어 한도를
늘려 우회하지 않았다.

all-logical의 실제 실행 순서 `A M M A M A`:

| 변형 | 모든 first canonical terminal 표본 | 중앙값 |
| --- | --- | --- |
| A | 25,611.605 / 25,407.275 / 40,084.065ms | 25,611.605ms |
| M2 | 33,152.790 / 24,861.720 / 25,907.040ms | 25,907.040ms |

중앙값 차이는 약 1.15%이며 양쪽 모두 느린 표본이 남았다. 이 작은 표본으로
통계적 성능 동등이나 모든 기기의 overhead 부재를 증명했다고 하지 않는다.
확인한 구조적 사실은 **remote 6개 + controller proof 1개**, sampled active peak=7,
추가 full-CPU participant 없음이다. M2의 controller는 각각 445/452/454개 proof
task에 참여했고 remote는 234/235/220개를 처리했다. 총 25개 wave 중 각각
17/17/13개는 all-ready 전에 remote task를 발행했다. worker 준비 전체를 기다린
뒤에만 탐색하는 구조로 되돌아가지 않았다.

6워커 n-1에서 고친 시작 barrier와 달리, shared/all-logical에서는 기존 전역
repair를 publication 전에 유지한다. 두 작업을 controller에 동시에 넣어서
증명 hint를 잃거나 처리 용량을 초과하지 않는다. 이 경로의 serial repair까지
없앤 것으로 설명하지 않는다. peak count는 실제 CPU 사용률이나 모바일 실기
가속률을 의미하지 않는다.

## 제품 코드 검증과 승격 조건

- 최신 focused TypeScript 계약 5개 통과: worker budget, pool lifecycle,
  WASM wrapper, distributed runner, scheduling profile.
- early warm-before-ready, 모든 real receipt 회수, 2/6/7/11/32 워커 topology와
  memory-declined shared fallback을 계약으로 확인했다.
- Rust의 독립 정수 cut, warm receipt, 원본 canonical parity 테스트를 추가했다.
  로컬 native 실행은 하지 않았으며, 별도 `Minimum Hotfix Contracts` CI에서
  제품 코드만 확인한다. CI 결과는 이번 턴에서 기다리거나 조회하지 않는다.
- 외부 Qnia 코드/모델/벤치마크는 CI에 넣지 않는다. 이 CI는 source acceptance,
  배포 승인, Pages/Oracle/Cloud Run mutation 권한이 없다.
- GUI host와 WASM ABI는 한 source로 함께 재빌드/게시해야 한다. 새 overlap
  프로토콜을 모르는 이전 host와의 혼합을 승인하지 않는다. 버전이 섞인 bootstrap,
  실제 S23 Ultra의 메모리·thermal 환경, 더 큰 fixture 회귀는 승격 전 후속 검증이다.
- M1 커밋을 단독 승격하지 않는다. G는 기본 비활성으로 유지한다. M2의 후보
  브랜치를 현행 안정 배포 소스로 병합하지 않았고, 3초 목표도 미달이다.

## 동일성, lazy 정책, 검증의 한계

세 binary 실험의 성공 표본 **19개**는 모두 같은 원본 ID 25개와 normalized solution key를 반환했다.
기존 source-bound matrix의 5,040개 큐 전부를 각 결과 집합으로 다시 커버했다.
known alternatives=1, total alternatives=null, enumeration_complete=false이며
후속 tie 페이지를 요청하지 않았다. 최초 집합 이후 숨은 전체 tie 열거를 계측에
포함하지 않는다.

- canonical members JSON SHA256: `7314db27276521fe547b76236fd196726c7a40cbc4ef2af2935ea2363df04d8c`.
- product set identity: `cf71d2f992b9baa379d3a44e1de481b0f5743e8fc9811dda40ce5b9be982eba3`.
- candidate map identity: `abaaba4a4fabe14617fdb8fabe6bf1c92125f285002f00d880e2527218716c8d`.

원본 큐 커버와 기준선의 canonical 일치는 이 fixture의 회귀 근거이다. 모든 입력에
대한 독립 수학적 증명, mobile thermal/메모리 환경의 재현, 실제 Pages 배포 성공을
의미하지 않는다. sampled active count도 CPU 사용률은 아니다.

## 로컬 원시 근거와 후속

- `_local/reports/minimum-browser-ab-20260907/`: 최초 2×2 비교 4표본.
- `_local/reports/minimum-browser-ab-20260908/`: M2 초기 6표본, 실패 포함.
- `_local/reports/minimum-browser-ab-20260908-v3/`: 최종 6워커 4표본과 all-logical
  6표본, 명시 옵션 누락으로 거부된 입력 1개. 그 거부와 초기 M2 오류까지 총 2개
  비성공 표본도 보존했다.
- `_local/research/summarize-minimum-browser-ab-20260908.mjs`: matrix 검증, 전 큐
  재커버, first canonical 일치 및 lazy 상태 검증. CI/제품에서 import하지 않는다.
- `qnia-minimum-variance-ab-2026-09-08.md`: 동일 kernel CP-SAT 12표본과 편차 분석.
- `v080-minimum-algorithm-hotfix-ab-2026-09-07.md`: 더 강한 proof/학습 및 canonical
  질의 간 재사용의 후속 구현 기틀.

새 배포 실행은 결과를 기다리지 않았으며 본 로컬 연구로 배포 통과를 대체하지 않는다.

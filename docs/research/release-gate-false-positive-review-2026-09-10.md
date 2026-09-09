# 릴리스 게이트 거짓 차단 검토 — 2026-09-10

검토 기준은 main `972f2ebd9f9e2fefb9b441e6ff2e983a16cd19e8`이다.
이번 변경은 오류 문구 1개와 테스트 제목 13개의 불필요한 문자열 검사를
제거한다. 테스트는 추가하지 않는다. 아래 후속 제안은
현재 배포 승인 조건을 변경하는 결정이나 구현 완료를 뜻하지 않는다.

## 확인된 실패와 수정

[canonical CI 34395943949](https://github.com/daejunnom/Clearra/actions/runs/34395943949)의
실제 실패 지점은 `release-acceptance-foundation-no-product-debt`의
`Release Identity Gate`다. 로그의 유일한 architecture error는 다음과 같다.

```text
Production observation evidence is missing 'production surface probe output is not canonical JSON'
```

이 검사는 `scripts/architecture/validate_release_static_contract.ps1`에서
오류 메시지의 전체 문장을 소스 문자열로 검색했다.
직전 변경은 `observe-production-surfaces.mjs`의 진단에 서비스 이름을 넣도록
오류 문장을 `${label} output is not canonical JSON`으로 바꿨다.
JSON을 파싱하고 정규 직렬화 결과와 비교해서 거부하는 실제 코드는 유지됐다.
따라서 이번 실패는 제품 동작의 회귀가 아닌 정적 검사 자체의 거짓 차단이다.

다른 실행 job의 제품/Rust/WASM/Discord 검증은 성공했다.
`release-failure-summary` 실패와 최종 acceptance 생략,
[Pages queue 34395973908](https://github.com/daejunnom/Clearra/actions/runs/34395973908)
실패는 이 선행 실패의 전파다. 각각을 별도 제품 결함으로 세지 않는다.

이번 수정:

- 해당 오류 문장의 소스 검색 요구를 제거했다.
- 기존 canonical 회귀 테스트 소유자가 실행하는
  `observe-production-surfaces.test.mjs`의 테스트 제목 13개를 다시 소스에서
  검색하던 중복 검사를 제거했다. 해당 파일의 실제 실행 소유권 검사는 유지했다.
- JSON 출력 변형마다 테스트를 추가하는 방안은 채택하지 않았다.
  제품 로직은 바뀌지 않았고, 이번 결함을 해결하는 데 새 테스트가 필요하지 않다.
- 기존 정상 출력 수락 테스트, SHA-256/출처/시간/서비스 일치 검증은 유지했다.
- 이미 main에 들어온 독립 실패 수집 기능을 아직 미반영 브랜치로 설명하던
  `docs/release-blocking-rules.md`의 오래된 설명을 정정했다.

로컬 검증은 다음과 같이 통과했다. 전체 제품 승인 증거는 새 canonical
workflow가 별도로 생성해야 한다.

```text
node scripts/tools/run-focused-js-tests.mjs scripts/release/observe-production-surfaces.test.mjs scripts/tools/validate-release-cli-smokes.test.mjs
140 passed, 0 failed

pwsh -NoProfile -File scripts/validate_architecture.ps1 -TaskName 'Release Identity Gate' -QuietProgress
exit 0

node scripts/tools/validate-release-cli-smokes.mjs
Release CLI smoke contract passed.
```

## 배포 비용과 구조적 단순화

완료된 CI 34395943949의 Jobs API에서 확인한 실행 시간은 다음과 같다.
한 번의 실행 표본이며, 병렬 실행된 job 시간을 직렬 전체 시간이나 과금액으로
합산하지 않는다. 빌드와 테스트가 같은 step에 있으면 개별 비용으로 분리할 수 없다.

| 실행 범위 | 경과 시간 | 해석 |
| --- | ---: | --- |
| metadata job 전체 | 32초 | 현재 55개 회귀 테스트 파일을 포함하는 job |
| GUI 빌드 step | 906초 | 약 15분; 이 표본의 큰 비용 |
| RustExact shard 실행 step | 819초 | 약 14분; 빌드/검증을 포함한 단계 |
| Windows CLI 빌드·실행 step | 761초 | 약 13분 |
| WASM producer 빌드 step | 474초 | 약 8분 |
| Linux CLI 빌드 step | 459초 | 약 8분 |

따라서 문자열 검사를 없애는 것은 유지보수와 거짓 차단을 줄이는 수정이지만,
그 자체로 빌드 시간 대부분을 없애지는 못한다. 비용을 줄이려면 다음 순서로
검사의 역할과 제품 빌드 단위를 단순화하는 편이 효과적이다.

1. **표현을 고정하는 검사부터 삭제한다.** 오류 문구, 테스트 제목,
   내부 함수 배치 등은 공개 제품 계약이 아니므로 보통 배포 조건일 필요가 없다.
   실제 출처/권한/제품 동작을 확인하는 기존 검사와 같은 내용을 중복 보장한다면
   해당 검사를 제거한다. 제거한 검사마다 새 테스트를 만드는 규칙은 두지 않는다.
2. **하나의 요구사항에는 실행 소유자를 하나 둔다.** 정적 검사,
   workflow 문자열 테스트, 실제 동작 테스트가 같은 규칙을 반복 보장하는지
   살펴보고 가장 직접적인 검증만 남긴다. 실패를 실제로 잡은 이력이나
   고유한 계약이 없는 테스트는 제거 또는 통합 후보로 분류한다.
3. **배포와 무관한 개발 도구 검증을 canonical 경로에서 분리한다.**
   현재 55개 파일에는 candidate preflight, 로컬 GUI 실험 도구, Windows
   watchdog의 테스트도 포함된다. 제품/배포 의존성 여부를 확인한 뒤 해당
   도구가 변경되는 PR/개발 단계로 이동할 후보들이다. 보안 경계까지 이름만 보고
   일괄 제거하지 않는다. 이 조정의 주요 효과는 결합도 감소이며, 32초짜리
   metadata를 줄이는 것만으로 큰 빌드 비용 절감을 기대하지 않는다.
4. **값싼 배포 계약 검사를 비싼 빌드 전에 배치한다.** 현재 오류는
   NoProductDebt에서 발생했지만 다른 제품 빌드는 계속 끝까지 실행됐다.
   최소한의 출처/권한/워크플로 계약을 하나의 사전 단계로 이동하면 이미 배포가
   불가능한 상태에서 제품 빌드를 시작하는 일을 줄일 수 있다.
   같은 검사를 사전 단계와 후속 단계에서 반복하지 않는 것이 조건이다.
   사전 단계 성공 뒤의 독립 기능 실패 수집은 계속 유지할 수 있다.
5. **변경되지 않은 제품을 다시 빌드할 필요가 없는 배포 모델로 바꾼다.**
   현재 정책은 release infrastructure 등 작은 변경도 full로 승격하고,
   compiled WASM identity에 저장소 SHA가 들어 있어 서로 다른 커밋 간 산출물
   재사용이 막힌다. 장기적으로 제품별 입력(소스/의존성/도구체인/스키마)의 digest와
   릴리스 manifest의 저장소 SHA를 분리하고, 바뀐 제품만 빌드·검증하는 모델을
   검토한다. 기존 Fast Fix는 qualification-only이므로 먼저 실제 배포가 검증할
   component ledger와 receipt 연결이 필요하다. 지금의 검증자를 건너뛰고 다른
   SHA의 산출물을 재사용하는 방식으로 구현하면 안 된다.

이 작업은 전체 테스트를 일정 비율 삭제하는 작업으로 시작하지 않는다.
작은 계약별로 "직접 검증 1개 / 중복 검사 / 개발 전용 / 고비용 통합 검증"을
분류하고, 중복과 개발 전용 항목부터 canonical 경로에서 줄인다.
테스트 수를 새로 늘리는 것보다 전체 배포가 의존하는 조건의 수를 줄이는 것이
이 검토의 주된 방향이다. 위 2~5번 구조 변경은 이번 배포에 구현하지 않았다.

## 추가 운영 개선 후보

### 1. 오류 문장과 테스트 제목을 배포 계약에서 분리

우선순위: 높음. 이번에 관련 문자열 검사 14개를 제거했고, 다른 구간은 후속 정리가 필요하다.

`validate_release_static_contract.ps1`의 다른 구간에는
오류 문장과 테스트 제목의 정확한 문자열 검사도 남아 있다.
진단 개선이나 테스트 이름 변경이 실제 안전성과 무관하게 배포를 막을 수 있다.

정적 검사는 실행 소유자, 의존성, 권한, 공개 스키마, 금지 API처럼 구조적으로
검증할 수 있는 경계에 집중한다. 동작 보장은 기존 기능 테스트의 소유자를
사용하고 동일한 내용을 검증하는 새 테스트를 추가하지 않는다.
YAML 검사도 권한/조건/의존성을 보존하면서
동등한 표현 변경을 허용할 수 있는지 개별 검토한다.

전환의 완료 기준은 두 가지다. 오류 문구나 테스트 이름만 바꾸면 통과하고,
실제 거부 분기를 없애거나 승인 출처를 바꾸면 테스트가 실패해야 한다.
문자열 검사를 일괄 삭제하는 방식은 피한다.

### 2. 외부 접근 검증을 변경 작업 전에 수행

우선순위: 높음. 후속 제안.

`.github/workflows/discord-deploy.yml`의 global sync 단계는 현재
`GH_TOKEN`의 존재를 먼저 확인한다. 그러나 값이 존재하는 것만으로 그 job에서
필요한 Pages/Actions 조회가 실제로 허용된다는 사실까지 증명하지는 못한다.

같은 job의 실제 권한으로 필요한 읽기 API를 미리 호출하는 작은 사전 점검을
두고, 대상 repository/run/deployment를 읽을 수 있는지 검증한다.
단계별 권한 설정과 자식 프로세스에 전달되는 인증 환경변수도 회귀 테스트한다.
실패하면 변경 작업 전에 인프라/권한 실패로 끝내어 불필요한 rollback을 줄인다.
사전 점검은 이후의 출처/해시 검증을 대체하지 않는다.

### 3. 읽기 API의 일시적 실패만 제한적으로 재시도

우선순위: 높음. 후속 제안.

`production-surface-probe-adapter.mjs`의 `fetchJsonBounded`는
현재 한 번의 GET 실패나 비정상 HTTP 응답에서 즉시 예외를 낸다.
이를 제품 불일치와 전송 실패로 구분하면 일시적인 외부 장애가 전체 재배포로
이어지는 일을 줄일 수 있다.

멱등 GET의 연결 실패, 일시적인 502/503/504, 명시적인 rate limit만 대상으로
재시도 횟수와 전체 시간 예산을 제한한다. rate limit은 응답의 대기 시간을
준수하고, 예산보다 길면 명확한 인프라 실패로 종료한다.
단순한 403을 모두 rate limit으로 취급하지 않는다. GitHub도
`Retry-After`/`x-ratelimit-reset` 준수와 유한한 재시도를 권고한다.
[GitHub REST API 권고](https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api#handle-rate-limit-errors-appropriately)

실제 SHA/해시/권한/스키마 불일치, malformed JSON, 제품 테스트 실패에는
이 재시도를 적용하지 않는다. 배포나 Discord sync 같은 변경 API도 자동 반복하지
않는다. 각 시도와 최종 원인을 기록하고, 관측 시간과 freshness 조건은 실제로
성공한 표본을 기준으로 검증한다. 테스트가 성공할 때까지 반복하는 정책과 분리한다.

### 4. 비배포 qualification의 실패 후 재실행 규칙 일치

우선순위: 중간. 후속 제안.

`docs/test-policy.md`와 `canonical-acceptance-run.mjs`는 성공한 승인 run이
없는 실패한 SHA에 대해 새 canonical dispatch를 허용한다.
반면 `fast-fix-qualification.yml`은 이전 실행이 실패했어도 같은 SHA의
새 qualification을 막는다. 비배포 점검의 일시적 인프라 실패에도 새 소스
커밋을 요구하는 것은 검토할 만한 불일치다.

이전 실행이 종료됐고 승인 성공/진행 중 실행/운영 변경이 없음을 확인한 경우에만
새 attempt-1 dispatch를 허용하는 방향을 검토한다. 과거 실행 이력과 각
run/attempt의 증거를 분리해서 보존하고, 실패한 run의 부분 결과를 승인으로
승격하지 않는다. 이번 수정에서 이 정책은 변경하지 않았다.

### 5. 승인 증거의 보관 수명을 배포·복구 수명과 맞춤

우선순위: 중간. 후속 제안.

현재 정책은 성공한 canonical SHA를 다시 승인할 수 없으며, 증거가 만료되거나
사라지면 새 소스 커밋과 전체 게이트를 요구한다.
`canonical-acceptance-run.mjs`도 온라인 run/attempt 이력을 조회한다.
제품이 그대로여도 증거 보관 문제 때문에 재빌드가 필요해지는 구조다.

먼저 핵심 승인 산출물의 보관 기간을 명시하고 지원/rollback 기간과 비교한다.
이미 있는 checkpoint/rollback receipt 체계를 검토하여, 원래 승인한
run/attempt 메타데이터, 정확한 산출물, 해시를 함께 영구 보관하는 경로를 설계한다.
만료된 근거를 무시하거나 로컬에서 승인 증거를 새로 만드는 방식은 허용하지 않는다.
별도 보관 자료를 실제 승인에 사용할 때에는 검증자와 권한 모델의 명시적인
설계 변경이 필요하다. 자료가 없으면 계속 차단해야 한다.

GitHub는 artifact 만료일을 `expires_at`으로 제공하며, 삭제된 artifact는
복원할 수 없다고 설명한다.
[GitHub artifact 보관 문서](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/remove-workflow-artifacts)

## 유지할 조건과 평가 방법

실제 제품 오류, 메모리 안전성, 정확성 회귀, 산출물 변조, 승인되지 않은
출처/권한, rollback 불가능 상태에 대한 차단은 유지한다.
독립 실패 수집, 단일 WASM 생산자, 승인된 동일 SHA 산출물 재사용은 이미
구현돼 있으므로 새 개선으로 중복 제안하지 않는다.

거짓 차단과 인프라 실패를 구분해서 기록한다. 이번 사례는 거짓 차단이지만,
토큰 누락은 파이프라인 설정 결함이고 Discord 명령어 크기 제한 초과는
실제 API 계약 위반이다. 모두 게이트를 약화해서 해결할 문제는 아니다.

후속 변경의 효과는 실행 시간만으로 판단하지 않는다. 원인별 차단 횟수,
변경 작업 전에 발견한 권한 오류 수, 재실행 없이 해결된 전송 실패 수,
그리고 변조/오류를 여전히 차단하는 mutation test 결과를 함께 비교한다.
요청에 따라 이번 재배포를 계속 조회하거나 별도 모니터링을 만들지는 않는다.

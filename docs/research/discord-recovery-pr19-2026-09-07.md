# Discord 복구 통합과 제한된 태그 정리 — 2026-09-07

## 변경 범위와 근거

PR #18의 1초 GitHub 생성/시작 timestamp 역전 처리와 회귀 테스트,
canonical release test manifest 등록을 PR #19로 그대로 통합한다.
#18의 기준 head는 `0c92db5aa21d39ee1015cd64e905059d70512682`다.

사용자는 GCP GUI에서 이전 리비전 `clearra-current-job-v075-701454b`가
100%, 후보 `clearra-current-job-v080-9177273`이 0%이며 최신 생성
리비전이라고 확인했다. 생성 후 약 5일이라는 관측은 새로운 복구 권한이나
검증 기한 연장의 근거로 사용하지 않는다. 후보 태그의 현재 존재 여부,
실제 IAM 허용 여부, 이미지 digest는 실행 시 API와 봉인된 intent로 확인한다.

원래 복구 대상은 배포 `33583378208/1`이고, 소스는
`91772735c3f7ec7d89ecd3e82aa4af4014995bf6`이다. prestage artifact
`9829463804`의 ZIP digest는
`sha256:d3de9dbe64696983d911944f2c1b096d73271b42b561b22a3a5519143f1ce679`다.
복구 `34089672723/1`은 rollback identity로 후보 태그 제거 시 actAs 거부를
받았다. 현재 runtime은 `clearra-current-job`이며, 잘못 안내됐던
`clearra-discord-runner`나 `clearra-discord-worker`를 만들거나 참조하지 않는다.

## 실제 수정

기존 복구의 후보 태그 제거 한 지점만 Cloud Run v2 PATCH로 제한한다.
전체 Service 또는 revision template을 다시 제출하지 않으며, 본문은
`name`, `etag`, `traffic`, updateMask는 `traffic`뿐이다. `allowMissing`,
`forceNewRevision`, `template`, `serviceAccount`, IAM 변경은 보내지 않는다.

처리 순서는 다음과 같다.

1. 기존 run/attempt/artifact/intent 검증을 통과한 원래 복구 owner가 호출한다.
2. 별도 helper가 기존 canonical intent 검증을 재사용한다.
3. 같은 rollback credential로 서비스와 후보 이미지·최신 리비전을 조회한다.
4. 이전 리비전 100%, 후보 0%, 후보 태그 소유권, desired/status 라우팅 일치를 확인한다.
5. 태그가 이미 없으면 쓰지 않는다. 있으면 정확한 후보 항목 하나만 제외한다.
6. `validateOnly=true`로 검증하고 서비스·리비전을 재조회해 preimage가 같은지 확인한다.
7. 기존 etag로 실제 PATCH를 한 번 수행하고 작업 완료와 독립 readback을 확인한다.
8. 기존 PowerShell의 v1 readback, Oracle 정리, 결과 증거 봉인은 그대로 진행한다.

서비스 계정, template, 보안 설정과 다른 태그를 바꾸는 readback은 실패다.
권한 거부, 충돌, 알 수 없는 operation, 1MiB 초과 응답, 네트워크 오류는
복구 실패로 남는다. token이나 서비스 전체 payload를 로그로 출력하지 않는다.
클라이언트의 요청당 timeout은 20초, operation poll은 최대 60회이며 전체
진행 deadline도 검사한다. 외부 workflow의 기존 제한된 재시도만 유지한다.

**v2 traffic-only 요청이 실제 프로젝트에서 no-actAs로 허용되는지는 아직
라이브 검증 전이다.** validateOnly 성공도 실제 write 성공 또는 복구 완료
증거가 아니다. 같은 403이 발생하면 권한을 넓히거나 deployer/Oracle로
재인증하지 않는다. 이 PR은 no-actAs 우회나 보장된 배포 성공을 주장하지 않는다.

`live` 단계의 기존 `--to-revisions` 트래픽 복구 호출은 이번 수정 대상이
아니다. 관측된 prestage 태그 정리 외의 모든 장애 유형이 해결됐다고 주장하지 않는다.

## 유지되는 계약

`github-wif-bootstrap.test.mjs`의 runtime actAs 금지 부정 테스트와 IAM
bootstrap을 변경하지 않는다. 후보가 최신이어야 하는 잔여 상태 계약, 원래
SHA·attempt·artifact digest, 보호된 Environment 및 recovery debt 판정도 유지한다.
새 PR 테스트 workflow는 contents read만 가지며 mock/로컬 회귀 테스트만 한다.
운영 승인이나 canonical acceptance artifact를 만들지 않는다.

## 워커 정책: 소스 확인과 후속 A/B만

`native_spin_structure_execution.rs`는 요청 N만큼 계산 스레드를 만들고,
호출자는 완료 조율만 한다고 명시한다. `native_forward_execution.rs`는
N-1 persistent worker와 호출자 coordinator로 구성한다. 따라서 전체
스레드 수와 동시에 CPU를 소모하는 계산 슬롯을 혼동하면 안 된다.

`parallel-product-audit-2026-09-06.md`도 별도 control-only coordinator를
허용하되 미승인 N+1 계산 역할은 추가하지 않도록 요구한다. 이는 사용자가
설명한 관리 역할 분리와 부합한다. 다만 startup의 `9 worker(s)` 문자열
하나가 모든 제품의 '8 계산 + 1 관리자'나 모든 계층의 work-stealing 구현을
직접 증명하지는 않는다. 이 PR은 어떤 worker/runtime/solver 소스도 변경하지 않는다.

후속 A/B에서는 N 계산+별도 관리 / N-1 계산+관리, 관리자 계산 참여 여부,
작업 재배분·훔치기 빈도, 실제 admitted compute slots, tail latency, peak RSS를
동일 query·binary·CPU quota·정답으로 비교한다. 재현된 성능 근거 이전에는
9라는 로그만으로 오류 판정이나 운영 설정 변경을 하지 않는다.

## 병합 후 실제 복구 실행

로컬 gcloud 설치는 필요 없다. GitHub Actions의 기존 보호된 workflow가 설치한다.
PR 회귀 테스트를 확인한 뒤 #19를 병합하고 다음 **새 실행**을 만든다.
기존 실패 run의 단순 rerun은 새 trusted-helper SHA 검증을 대신하지 않는다.

GitHub Actions → Recover Discord Production → Run workflow:

- Branch: `main`
- original_run_id: `33583378208`
- original_run_attempt: `1`
- expected_current_main: 병합된 현재 main의 전체 40자리 SHA

GitHub CLI가 있는 환경에서는:

```bash
MAIN_SHA="$(gh api repos/daejunnom/Clearra/git/ref/heads/main --jq .object.sha)"
gh workflow run discord-deploy-recovery.yml --repo daejunnom/Clearra --ref main \
  -f original_run_id=33583378208 -f original_run_attempt=1 \
  -f expected_current_main="$MAIN_SHA"
```

`discord-runtime-rollback` 승인은 기존대로 진행한다. authority만 녹색이고
recover가 skipped인 별도 no-op은 이 원래 실패의 runtime 복구 증거가 아니다.
원래 부모에 연결된 terminal result가 검증·업로드된 뒤에만 새 main의
canonical acceptance와 그 SHA의 Discord 배포를 진행한다. 직접 업로드,
수동 승격, recovery debt 삭제로 순서를 우회하지 않는다.

공식 API 근거:
https://docs.cloud.google.com/run/docs/reference/rest/v2/projects.locations.services/patch
https://docs.cloud.google.com/run/docs/reference/rest/v2/projects.locations.services

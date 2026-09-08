# Discord 복구: 확인된 actAs 거부와 실패 경계

## 실제 원인과 남은 운영 조건

재시도 `34132011709/1` / recover job `101774229266`은
`9fae522ce5d222a6330c9d61b2e761d369fe4342`에서 실행됐다.
세 번 모두 `PATCH HTTP 403`, `phase=validate`,
`google_status=PERMISSION_DENIED`,
`reported_permission=iam.serviceAccounts.actAs`였다.

이는 실제 apply 이전의 traffic-only v2 검증도 기존 rollback 자격으로
거부됨을 보여준다. `updateMask=traffic`은 갱신 필드 제한이지 IAM 면제가
아니다. 이전의 v2 경로가 no-actAs로 성공할 수 있다는 가설은 이 프로젝트의
이번 실행에서 기각됐다. 계정 부재나 다른 이름의 runtime을 추정하지 않는다.

`reason=unknown`은 허용 목록의 ErrorInfo.reason을 확보하지 못했다는 뜻이며,
권한 이름까지 모른다는 뜻이 아니다. 오류 본문은 비밀값 때문에 원문을 남기지
않으므로 실제 ErrorInfo 누락과 파싱 실패는 앞선 로그만으로 구분할 수 없었다.

## 코드 수정 범위

- HTTP 오류에 body_state와 permission_source/error_info를 추가한다.
  본문 없음, JSON 실패, 형식 불일치, 16KiB 초과, 읽기 실패를 구분한다.
- 알려진 403 actAs 거부에만 runtime-actas-denied를 진단하고 전용 실패 코드
  77로 전달한다. Google reason을 만들어 내지 않는다.
- PowerShell은 위 helper의 77만 명시적인 runtime actAs 거부로 다시 던진다.
  다른 실패도 helper 종류와 종료 코드가 남는다. stderr는 계속 전달하고,
  stdout은 함수 반환값 오염을 막기 위해 계속 폐기한다. raw 인자는 출력하지 않는다.
- prestage에서는 원래 prior 100% 확인 후, Oracle freeze/cleanup보다 먼저
  candidate 태그 제거 validate-only를 수행한다. 실패 시 이 invocation의 Oracle
  원격 작업은 시작하지 않는다. 외부 workflow의 기존 재시도 정책은 변경하지 않는다.
- 실제 Cloud 정리와 증거 봉인은 기존 위치에 남아 있으며 preflight 성공을
  믿고 검증을 생략하지 않는다. live 단계의 보상 순서는 변경하지 않는다.

IAM/bootstrap/no-actAs 부정 테스트, Environment 승인, 원래 부모 run/attempt,
artifact digest, 배포 nonce, 최신 후보 검증, 최종 readback과 결과 검증은
변경하지 않는다. 워커/관리자/A-B/solver와 승인 자동화는 수정 범위가 아니다.

## 코드 수정만으로 해제되지 않는 조건

현재 권한으로 태그를 제거하는 요청은 확인된 권한 거부를 다시 만난다.
새 진단/사전검사 CI 통과를 운영 복구나 배포 성공으로 해석하지 않는다.
태그가 남은 상태에서 같은 복구를 반복 실행하는 것을 해결책으로 제안하지 않는다.

원래 배포 대상은 `33583378208/1`이다. 사용자 확인 및 봉인된 입력에서:

- 서비스: clearra-current-job / clearra-cloud / asia-northeast1
- prior: clearra-current-job-v075-701454b (100%)
- candidate: clearra-current-job-v080-9177273 (0%, latest created)
- 정리 대상 태그: candidate-9177273

기존 권한이 있는 운영자가 현재 상태와 태그 연결을 재확인한 뒤 GUI에서 위
태그 하나만 제거하는 별도 운영 작업은 가능하다. 이것을 workflow의 자동
배포 계정 재인증으로 구현하거나 rollback에 actAs를 추가하지 않는다.
리비전 삭제, 새 리비전 생성, latest 승격, 다른 태그/traffic/IAM 변경은 하지 않는다.
태그 편집의 Save 권한이 없으면 중단하고 별도로 요청해야 한다.

태그가 없어진 뒤 새 main SHA로 기존 보호된 recovery를 새로 실행한다.
기존 함수는 태그가 없으면 Cloud PATCH를 건너뛰지만 prior 100%, 후보 이미지,
후보 0%·tagless·latest와 Oracle 상태, 결과 증거는 여전히 검증한다.
사람의 관측이나 위 문서 자체는 recovery clearance가 아니다.
복구 완료 artifact가 검증·업로드된 뒤에만 정식 acceptance/배포로 진행한다.

장기적으로 자동 traffic 복구와 no-actAs 정책의 충돌은 별도 설계 사항이다.
한 번의 수동 태그 정리는 앞으로 모든 live rollback이 자동으로 가능해졌다는
증명이 아니다. 이번 수정은 그 정책을 조용히 변경하지 않는다.

## 외부 근거

- 실제 CI: https://github.com/daejunnom/Clearra/actions/runs/34132011709/job/101774229266
- Cloud Run 요구 역할 및 태그 제거:
  https://docs.cloud.google.com/run/docs/rollouts-rollbacks-traffic-migration
- updateMask / validateOnly 정의:
  https://docs.cloud.google.com/run/docs/reference/rest/v2/projects.locations.services/patch

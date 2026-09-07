# Discord 승인 대기 개선 — 별도 브랜치 준비

기준 main: `045a5d77ec1f3e77cadf3bdb7f993eb87e3a2dec`.
이번 배포의 운영 환경 설정, 대기 중 job 승인 또는 OIDC 정책은 변경하지 않는다.

현재 세 environment 모두 required reviewer 1명이 설정되어 있음을 API로
읽기 전용 확인했다. `promote`는 `discord-path-confirmation`, `sync-observe`는
`discord-global-command-sync`, 별도 recovery는 `discord-runtime-rollback`을
사용한다. 따라서 한 배포에 반복 승인 및 장애 복구 승인 대기가 생길 수 있다.
런타임 키의 scope와 GitHub environment reviewer는 서로 다른 보호 장치다.

사용자 확정: **배포당 최초 승인 1회 유지, 반복 승인만 제거**.
완전 자동 정책은 planner에서 거부한다.

| 정책 | promote | sync / observe | 소유권이 증명된 recovery |
| --- | --- | --- | --- |
| 최초 1회 승인 | 최초 승인 유지 | 추가 승인 없음 | 추가 승인 없음 |

선택은 완료됐다. 이번 요청은 별도 브랜치 준비이므로 운영에 적용하지 않는다.
environment 이름과 secret scope, main-only branch restriction, OIDC subject,
기존 wait timer 및 admin bypass 정책을 유지하는 정확한 전환 body만 계산한다.
특별한 custom protection rule을 발견하면 자동 제거하지 않고 계획을 거부한다.
planner는 읽기/쓰기 API, secret 조회, workflow approve API를 호출하지 않는다.

후속 통합은 다음 사항을 함께 충족해야 한다.

1. 현재 코드/테스트/문서의 reviewer-protected 표현과 실제 보호 계약을 일치시킨다.
2. source acceptance, recovery debt, artifact digest, shared production lock,
   변경 직전 authority 재확인 및 정확한 original attempt의 복구 증거는 유지한다.
3. 복구의 자동화는 이미 승인된 원본 전이 또는 소유권이 검증된 inactive cleanup에
   한정한다. review 제거를 임의 runtime write 권한으로 해석하지 않는다.
4. separate environment를 합쳐 SSH 권한을 approval-free candidate에 전달하지 않는다.
5. metadata-only CLI의 plan digest를 검토한 뒤 명시적 apply와 readback을 한다.
   불확실한 PUT 실패 시 기존 reviewer를 복원한다. 외부 정책 변경을 발견하면
   덮어쓰지 않고 미복원 환경을 명시한다. 최초 승인 환경에는 PUT 자체를 하지 않는다.
6. 반복 승인 제거와 별개로 workflow 전체 shared lock이 승인 대기 중 유지되는
   문제는 별도 평가한다. lock 범위를 임의 축소하면 승인 대기 동안 다른 배포가
   production을 변경할 수 있으므로 재검증/소유권 계약 없이 바꾸지 않는다.

준비 파일: `scripts/release/discord-approval-transition{,-plan}.mjs` 및 대응 test.
`node scripts/release/discord-approval-transition.mjs plan`은 세 환경의 protection과
branch policy만 읽고 전환 body 및 SHA256을 출력한다. 세 환경 모두 정확한
main branch 한 개만 허용함을 API로 확인했다. `apply <검토한-plan-SHA256>`은
후속 운영 전환용이며 이번 작업에서는 실행하지 않는다. Secret/approval API는
어느 모드에서도 사용하지 않는다. 계획 상태의 `ready_to_apply=false`는 승인된
배포 권한이 아니라는 뜻이며, 실제 apply 이후에만 `applied=true`가 된다. 이 커밋은
운영 자동화 전환 완료나 새 Discord 배포 성공을 의미하지 않는다.

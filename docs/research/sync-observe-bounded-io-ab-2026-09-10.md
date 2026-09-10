# sync-observe 종료 보장 및 읽기 병렬화

기준 커밋: `4d6cb7f7e68b67f79526c2c4faee8f85a160ea93`.
복구 변경은 이 커밋으로 main에 반영했고, 기존 Discord 실패 실행 34440716341의 복구 실행 34448584454가 성공했다. 개선 브랜치는 `codex/sync-observe-bounded-20260910`이다.

## 확정한 결함 수정

- Discord REST의 시간 제한을 헤더 수신부터 응답 본문/첨부 파일 소비까지 유지한다. 읽지 않는 오류 응답은 취소한다. 쓰기의 불명확한 실패에 대한 재전송 정책은 기존대로 유지한다.
- production HTTP helper는 본문을 읽는 동안에도 deadline과 바이트 상한을 적용한다.
- 공통 외부 명령 실행기는 소유한 프로세스 그룹에 TERM을 전달한 후 KILL로 전환하고, 출력 스트림 정리에도 상한을 둔다. Windows에서는 해당 PID의 하위 프로세스까지 종료한다. 부모가 취소되면 중첩 adapter가 자신이 시작한 프로세스에 취소를 전달한다.
- surface 하나가 실패하면 나머지 관찰도 취소하고 정리가 끝나기를 기다린다. 경과 시간은 monotonic clock을 사용하며 샘플 수에도 상한을 둔다. 증거의 UTC 시각 검증은 유지한다.
- Oracle 내부의 60초 작업과 75초 shell, 5초 종료 유예보다 짧던 외부 관찰 제한을 120초로 조정한다. 다른 surface는 최대 60초를 유지한다. 관찰 SSH 및 내부 동기 명령에는 90초 제한을 두고, 마지막 별도 Oracle 관찰도 120초로 제한한다.

## 실제 읽기 요청 A/B

2026-09-10 07:25:10 UTC에 완료한 수동 측정이다. Windows의 gcloud PowerShell launcher와 실제 Cloud API 및 두 health URL을 사용했다. Cloud 설정, 트래픽, 명령어 catalog를 변경하지 않았다.

A는 service → revision → stable health → tagged health를 직렬로 읽는다. B는 service/revision을 병렬로 읽고, 그 뒤 stable/tagged health를 병렬로 읽는다. 실행 순서를 AB, BA로 번갈아 가며 6쌍을 측정했다. 측정 전에 인증과 연결을 한 번 준비했다.

| 중앙값 | A 직렬 | B 병렬 |
|---|---:|---:|
| control-plane 읽기 | 9.959초 | 6.475초 |
| health 읽기 | 0.500초 | 0.258초 |
| 전체 읽기 | 10.410초 | 6.768초 |

전체 중앙값은 **34.985% 감소**했다. 6쌍 모두 B가 빨랐으며 모든 측정의 control-plane 응답 해시와 health 응답 해시가 각각 동일했다. 따라서 B를 채택했다. 구현에서는 control-plane 검증을 통과한 뒤 health 요청을 시작하며, 짝을 이룬 읽기 하나가 실패하면 다른 읽기도 취소한다.

이 수치는 Windows SDK 시작 비용과 해당 시점의 네트워크 상태를 포함한다. Ubuntu CI 전체나 20분 관찰 단계가 35% 빨라졌다는 뜻은 아니다. 수동 재현 도구는 `scripts/tools/benchmark-production-read-schedule.mjs`이며, 릴리즈 게이트에 추가하지 않았다.

측정 원본은 로컬 reports 디렉터리의 `sync-observe-read-ab-20260910.json`에 보존했다.

## 이벤트와 폴링의 역할

canonical acceptance → Discord 배포 → 실패 복구는 기존 GitHub `workflow_run` 이벤트를 이용한다. 이 이벤트는 불필요한 상태 조회 없이 다음 소유자를 시작한다. 외부 웹훅 수신 서버나 새 자격은 추가하지 않는다.

Pages 게시 큐는 정확한 승인 실행과 rollback capture를 기다릴 때 기존 bounded polling을 사용한다. 게시 실행의 정확한 dispatch 영수증을 확인하면 `publicationStatus: dispatched`로 끝난다. 이후 완료는 Pages 실행 자체의 상태와 증거가 소유하므로 중복 폴링을 제거했다. 큐 성공을 게시 성공으로 간주하지 않는다. 실제 Cloud/Discord/Oracle/Pages 상태의 증명도 읽기를 수행해야 한다. 완료 이벤트만으로 프로세스, revision, catalog 또는 캐시된 응답의 정확성을 대신 판정하지 않는다.

1,200초 관찰 시간, 마지막 Oracle 증거 제거, 권한을 여러 job으로 재구성하는 변경은 이번 읽기 성능 A/B의 동등성 범위에 포함되지 않는다. 특히 마지막 Oracle 결과를 제거하면 별도 종료 증거가 없어지고, 관찰 시간 단축은 릴리즈 수용 정책을 바꾸므로 시간 절감 수치만으로 채택하지 않았다.

## 확인

복구 변경은 기존 회귀 97개와 실제 실패 아티팩트의 원본 ZIP 해시 및 sync preimage 검증을 통과했다. 개선 변경은 HTTP, 관찰, authority/spec, 워크플로, 체크포인트 관련 검증을 통과했다. 프로세스 timeout/취소는 Windows와 Linux에서 실행했고, PowerShell 문법 및 실제 Oracle의 읽기 명령으로 변경한 SSH wrapper를 확인했다.

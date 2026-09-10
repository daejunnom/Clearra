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

1,200초 관찰 시간, 마지막 Oracle 증거 제거, 권한을 여러 job으로 재구성하는 변경은 위 읽기 성능 A/B의 동등성 범위에 포함되지 않는다. 특히 마지막 Oracle 결과를 제거하면 별도 종료 증거가 없어지고, 관찰 시간 단축은 릴리즈 수용 정책을 바꾸므로 시간 절감 수치만으로 채택하지 않았다.

## 관찰 중 실패와 증거 처리 실패의 후속 개선

추가 사용자 승인에 따라 같은 개선 브랜치에서 아래 변경을 적용한다. 성공을 위한 1,200초와 정확히 두 번의 authoritative four-surface 표본은 유지한다.

- `prepareDiscordProductionCheckpointInputs`는 시간 경과에 의존하지 않는 acceptance, source/run/attempt, recovery clearance, catalog preimage/readback, artifact metadata와 release artifact 검증을 소유한다. 동기화 직후 이 함수를 실행하고, 최종 checkpoint도 같은 함수를 다시 사용한다. 준비 검증은 관찰 파일이나 미래 job 완료를 요구하지 않는다. 과거 실패 실행 `34440716341/1`의 REST SHA-256을 확인한 보존 아티팩트로 실행했고, Windows CLI 시작 비용을 포함해 281ms에 통과했다. 네트워크나 운영 변경은 이 재현에 포함되지 않는다.
- 관찰 대기에는 60초 간격의 public HTTP guard를 넣는다. Cloud stable/tagged health와 Pages identity를 병렬로 확인한다. 같은 주소의 연속 통신 실패 두 번은 중단하며 성공하면 누적을 초기화한다. 정상 응답의 배포 identity가 달라지면 즉시 중단한다. 한 번의 GET은 10초 상한이다. 관찰 전체에서 최대 19회, GET 최대 57회이며 gcloud/SSH/Oracle Job이나 추가 권한을 요구하지 않는다. Oracle와 Discord의 상세 상태는 기존 시작/종료 표본에서 검증한다. 따라서 이 guard를 네 표면 전체의 연속 가동 증명으로 해석하지 않는다.
- guard는 실패만 판정하고 release 성공 증거를 생산하지 않는다. monotonic deadline을 고정해 guard 실행 시간을 다음 대기에서 차감하며, 취소 신호는 sleep과 활성 probe에 전달한다. 첫 guard에서 실패하는 주입 시계 회귀는 1,200초 중 60초에서 종료한다. 정상 guard가 매번 10초를 소비해도 성공 대기는 총 1,200초다. 이 값은 제어 흐름 검증이며 운영 MTTR 측정값이 아니다.
- 실패한 관찰은 기존 ERR/INT/TERM 경로에서 catalog를 먼저 복원한다. 실패 증거 업로드는 1분, checkpoint prerequisite 조회는 총 2분 및 개별 gh 요청 15초로 제한한다. 원래 실행이 종료되면 기존 `workflow_run: completed` 이벤트가 별도 runtime recovery를 시작한다. 복구는 남은 관찰 시간을 채우지 않지만, 기존 production 직렬화와 `discord-runtime-rollback` 승인 경계는 유지한다. 실행 중인 원본과 복구가 동시에 traffic을 변경하는 새 경로는 만들지 않는다.
- 관찰이 성공한 경우 sync/checkpoint 업로드는 최대 3회, 단계 전체 최대 5분이다. 최초 업로드 전에 허용된 JSON leaf들의 원본 bytes와 source/run/attempt/name을 묶고, 재시도 직전 같은 파일 집합과 digest인지 재검증한다. 모든 기존 입력, synchronized state의 bound files, 완료된 관찰 보고서와 현재 네 표면을 다시 확인해야 재시도한다. 관찰 종료 15분이 지난 증거, 누락/변경된 증거, 현재 상태 불명, 취소는 재시도를 허용하지 않는다. 복구용 첫 사전 아티팩트는 이 재시도 대상이 아니다.
- 재시도 중에는 release 확정을 보류한다. 재시도 업로드는 같은 실행의 같은 이름만 교체하고, 성공한 최종 artifact ID/digest만 반환한다. partial upload의 오래된 ID를 성공 영수증으로 재사용하지 않는다. 최종 봉인은 검증 오류를 즉시 실패시키며, `EIO/EBUSY/EAGAIN/EINTR/ETIMEDOUT`에 한해 live 재검증 후 최대 3회 로컬 I/O를 시도한다. 원자적 쓰기가 이미 완료됐으면 byte-identical 결과만 허용한다. 한도 초과나 재검증 실패는 단계 실패로 남겨 기존 catalog/runtime 복구를 실행한다. `continue-on-error`는 composite 내부 업로드에만 있으며 외부 단계 실패를 성공으로 숨기지 않는다.

운영 이벤트 수신 경로가 없는 HTTP 상태는 위의 제한된 폴링으로 보완하고, workflow 간 인계는 기존 이벤트를 사용한다. 새 웹훅 서버, IAM 역할, CI gate 또는 테스트 파일은 추가하지 않았다. 성공 시간만 비교하지 않고 검출 지연, 복구 시작 지연, 복구 완료 시간과 거짓 실패율을 구분한다. 60초 간격은 bounded fallback의 설정이며 운영 실패 분포에서 통계적으로 최적이라고 주장하지 않는다.

## 확인

복구 변경은 기존 회귀 97개와 실제 실패 아티팩트의 원본 ZIP 해시 및 sync preimage 검증을 통과했다. 개선 변경은 HTTP, 관찰, authority/spec, 워크플로, 체크포인트 관련 검증을 통과했다. 프로세스 timeout/취소는 Windows와 Linux에서 실행했고, PowerShell 문법 및 실제 Oracle의 읽기 명령으로 변경한 SSH wrapper를 확인했다.

후속 조기 실패/증거 재시도 변경은 관련 기존 테스트 파일 7개의 117개 검증을 통과했다. 5개 동작 사례를 기존 파일에 추가했으며 CI gate 목록은 바꾸지 않았다. JS syntax, composite YAML 구문, workflow actionlint도 확인했다. actionlint 1.7.12가 아직 지원하지 않는 기존 `concurrency.queue: max` 진단만 제외했고 해당 설정은 변경하지 않았다. 운영 중 업로드 장애를 인위적으로 발생시키거나 새 관찰/배포를 이 로컬 검증에서 실행하지 않았다.

# Security Policy

## 지원 범위

`main` 브랜치의 최신 버전을 보안 업데이트 대상으로 합니다. 실제 배포는 HTTPS, `SECURE_COOKIES=true`, 명시적 `ALLOWED_ORIGINS`, 비공개 PostgreSQL/Redis 네트워크를 사용해야 합니다.

## 취약점 제보

공개 issue에 악용 절차, 세션 토큰, 게임 데이터를 올리지 마세요. GitHub 저장소의 **Security → Report a vulnerability**를 통해 비공개로 아래 정보를 전달해 주세요.

- 영향받는 commit/배포 버전
- 재현 절차와 예상 결과
- 실제 영향과 가능한 완화 방안
- 비밀정보를 제거한 요청/응답 예시

제보를 확인하면 재현, 영향 평가, 수정, 검증 후 공개하겠습니다.

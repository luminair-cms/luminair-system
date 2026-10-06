# AR — Access Requests & Enrollment Specification

- **Prefix**: `AR`
- **Layer**: `application`
- **Unit under test**: `application::services::AccessRequestsService`
- **Format**: `U` (use case rules)
- **Test target**: `application/tests/ar_access_requests_service.rs`

---

## Vocabulary

| Term | Meaning / API symbol | Concrete representation |
|---|---|---|
| `Pending` | Request waiting for admin review | `AccessRequestStatus::Pending` |
| `Approved` | Request accepted with roles assigned | `AccessRequestStatus::Approved` |
| `Rejected` | Request declined with optional reason | `AccessRequestStatus::Rejected { reason: .. }` |
| `ActiveRequest` | Request that is either Pending or Approved | `request.is_active() == true` |
| `EnrolledUser` | User who has >= 1 `UserRoleAssignment` | `assignments.len() > 0` |
| `SelfReview` | Reviewer user ID equals the requester user ID | `caller.user_id == request.user_id` |

---

## Rules

### AR-01 — Submit Creates Pending Request For New User
- **Status**: `Draft`
- **Origin**: `adr:002`
- **Given**: A user with no existing role assignments and no active access requests.
- **When**: User submits via `service.submit(SubmitAccessRequestCommand::new(user_id, email, name)).await`.
- **Then**: Returns `Ok(request)` with `request.status == Pending`, `request.user_id == user_id`, and request is saved in the repository.

### AR-02 — Submit Rejected If User Already Enrolled
- **Status**: `Draft`
- **Origin**: `adr:002`
- **Given**: A user who already possesses at least one role assignment.
- **When**: User submits an access request.
- **Then**: Returns `Err(ApplicationError::Domain(DomainError::UserAlreadyEnrolled(user_id)))`.

### AR-03 — Submit Rejected If Active Request Exists
- **Status**: `Draft`
- **Origin**: `adr:002`
- **Given**: A user who already has a Pending or Approved request in the system.
- **When**: User submits another access request.
- **Then**: Returns `Err(ApplicationError::Domain(DomainError::AccessRequestAlreadyActive(user_id)))`.

### AR-04 — Submit Allowed If Previous Request Was Rejected
- **Status**: `Draft`
- **Origin**: `observed`
- **Given**: A user whose previous access request has status `Rejected`.
- **When**: User submits a new access request.
- **Then**: Succeeds with `Ok(new_request)` in `Pending` state.

### AR-05 — Approve Creates Role Assignments And Sets Status
- **Status**: `Draft`
- **Origin**: `adr:002`
- **Given**: A Pending access request, valid existing roles `[R1, R2]`, and an admin caller (`reviewer_id != user_id`).
- **When**: Admin invokes `service.approve(&admin, ApproveAccessRequestCommand::new(request_id, vec![R1, R2])).await`.
- **Then**: Returns `Ok(assignments)` creating assignments for `R1` and `R2`, sets `request.status == Approved`, and records `reviewed_by == Some(reviewer_id)`.

### AR-06 — Approve Rejects Self-Approval
- **Status**: `Draft`
- **Origin**: `observed` (Invariant: separation of duties)
- **Given**: A Pending access request submitted by `admin_user`.
- **When**: `admin_user` attempts to approve their own request.
- **Then**: Returns `Err(ApplicationError::Domain(DomainError::Unauthorized(_)))` stating reviewer cannot review or approve own request.

### AR-07 — Approve Rejects Non-Existent Role ID
- **Status**: `Draft`
- **Origin**: `observed`
- **Given**: A Pending access request, and role IDs including a random unknown UUID.
- **When**: Admin attempts approval.
- **Then**: Returns `Err(ApplicationError::NotFound { entity: "Role", .. })` before state mutation.

### AR-08 — Reject Records Reason And Reviewer
- **Status**: `Draft`
- **Origin**: `adr:002`
- **Given**: A Pending access request, and an admin caller (`reviewer != user_id`).
- **When**: Admin invokes `service.reject(&admin, RejectAccessRequestCommand::new(request_id, Some("Incomplete info"))).await`.
- **Then**: Returns `Ok(request)` with `request.status == Rejected { reason: Some("Incomplete info") }`, records `reviewed_by`, and saves in repository.

### AR-09 — Requester Can Read Own Request Without Admin Role
- **Status**: `Draft`
- **Origin**: `observed`
- **Given**: An existing request submitted by `user_a`, and caller `user_a` with no roles.
- **When**: Caller invokes `service.get_by_id(&caller, request_id).await`.
- **Then**: Returns `Ok(request)`.

### AR-10 — Non-Requester Denied Get Without ManageUsers Permission
- **Status**: `Draft`
- **Origin**: `adr:002`
- **Given**: An existing request submitted by `user_a`, and caller `user_b` without `ManageUsers` permission.
- **When**: Caller invokes `service.get_by_id(&caller, request_id).await`.
- **Then**: Returns `Err(ApplicationError::Unauthorized { .. })`.

---

## Open Questions

1. **Q1 (Re-submitting After Rejection)**: When re-submitting after rejection, does the system retain previous request history for audit? (Current behavior: previous request remains in table, query filters for active requests). Confirmed.

---

## Findings

| # | Location | Code Says | Doc / Spec Says | Impact |
|---|---|---|---|---|
| 1 | `domain/src/auth/access_request.rs:76` | Self-review blocked in domain aggregate | Separation of duties invariant | Protected against privilege escalation |
| 2 | `domain/src/auth/access_request.rs:88` | `MAX_ROLES = 50` limit on role assignment | Prevents unbounded payload allocations | Documented invariant |

# Frontend (React 19 + TypeScript) Rules

These non-negotiable rules and invariants govern all frontend code in `frontend/`, based on [`docs/ui-architecture.md`](../../docs/ui-architecture.md) and [`docs/api.md`](../../docs/api.md).

---

## 1. Strict TypeScript & Type Safety

- **Zero `any`**: The use of `any` is strictly prohibited. Use `unknown` with type guards or Zod runtime schema parsing.
- **Explicit Component Types**: All component props and custom hook return types must be explicitly typed using interfaces or type aliases.
- **Strict Compiler Compliance**: All code must pass `tsc --noEmit` and ESLint checks with zero errors or warnings.

---

## 2. Mantine v7 Design System Invariants

- **Standard UI Primitives**: All UI components must use Mantine v7 primitives (`@mantine/core`, `@mantine/hooks`, `@mantine/form`, `@mantine/notifications`, `@mantine/modals`, `@mantine/dates`, `@mantine/tiptap`).
- **Zero Raw CSS / Inline Styles**:
  - Do not write raw CSS files or inline `style={{ ... }}` objects when Mantine style props or theme tokens exist.
  - Colors, spacing, radii, typography, and dark/light mode adaptations must reference the centralized `theme.ts`.
- **System Notifications**: Display toasts and system alerts exclusively through `@mantine/notifications` (`notifications.show(...)`).

---

## 3. Server State vs. Client State

- **Server State via TanStack Query v5**:
  - All data fetching, caching, and mutations must go through TanStack Query v5 custom hooks located in `api/*.api.ts`.
  - **Never use `useEffect` for data fetching or mutation orchestration.**
- **Centralized Query Keys**:
  - Query keys must be defined as tuple factory functions in `api/query-keys.ts` (e.g. `queryKeys.documents.detail(type, id)`). Never construct raw query key strings inline in components.
- **Client State via Zustand**:
  - Global client state (auth tokens, current user claims, layout state, draft editing buffers) must live in dedicated Zustand stores (`useAuthStore`).
  - Local component state is managed with React `useState` or Mantine form state.

---

## 4. Routing & Search Parameters

- **Type-Safe Routing**: Use TanStack Router for all application navigation and route definitions.
- **Search Parameter Validation**: All URL query parameters (filters, pagination, sort keys) must be validated with Zod schemas in route definitions via TanStack Router's `validateSearch`.

---

## 5. RFC 9457 Error Handling & Localization (i18n)

- **Problem Details Interception**:
  - The centralized Axios client (`api/client.ts`) must intercept all non-2xx responses and normalize them into RFC 9457 `ProblemDetails` structures.
- **Frontend Owns Localization**:
  - The UI client owns end-user localization. Always map `error.code` to the translation catalog: `t('errors.' + error.code, error.details)`.
  - **Never display the raw backend developer `detail` string directly to the user as localized copy.**
- **Form Field Errors**:
  - Endpoint validation errors (`422 Unprocessable Entity` containing `invalid_params`) must be mapped directly into `@mantine/form` field error states (`form.setErrors(...)`).

---

## 6. Component Architecture & Metrics

- **Component Size**:
  - Target **$\le 50$ lines** per component.
  - When a component exceeds 50 lines, decompose it by extracting sub-components, custom hooks, or utility helpers.
- **JSX Nesting & Readability**:
  - Maximum nesting depth of 2 levels for conditional branches or loops within JSX. Extract complex conditional branches into dedicated child components.
- **Single Responsibility**:
  - Presentational components focus solely on rendering UI and firing callbacks.
  - Custom hooks or container components encapsulate API calls, query mutations, and state management.

---

## 7. Newspaper Code Layout (TypeScript & React)

Every source file (`.ts`, `.tsx`) must follow top-down dependency ordering:
1. **Imports**: External libraries/frameworks (`react`, `@mantine/*`, `@tanstack/*`) → internal shared aliases (`@/api`, `@/components`) → local relative imports (`./helpers`)
2. **Types & Interfaces**: Component prop types, form values interfaces, and local state types
3. **Primary Export**: The main component, custom hook, or service entry point appears at the top
4. **Internal Helpers & Sub-Components**: Private leaf rendering functions, specialized child sub-components, and transformation helpers follow below
5. **Constants & Static Metadata**: Default values, static options, and style configs at the bottom

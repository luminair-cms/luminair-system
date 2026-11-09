import type { ReactElement } from 'react';
import { Alert, Button, Group } from '@mantine/core';
import { IconAlertCircle } from '@tabler/icons-react';
import { useTranslation } from 'react-i18next';
import type { ProblemDetails } from '@/api/errors';
import { getLocalizedErrorMessage } from '@/i18n/error-mapper';

// ---------------------------------------------------------------------------
// Types & Interfaces
// ---------------------------------------------------------------------------

export interface ErrorAlertProps {
  error: ProblemDetails;
  title?: string;
  onRetry?: () => void;
  className?: string;
}

// ---------------------------------------------------------------------------
// Primary Export
// ---------------------------------------------------------------------------

export function ErrorAlert({
  error,
  title,
  onRetry,
  className,
}: ErrorAlertProps): ReactElement {
  const { t } = useTranslation();
  const message = getLocalizedErrorMessage(error, (key, opts) => (opts ? t(key, opts) : t(key)));
  const displayTitle = title ?? error.title;

  return (
    <Alert
      icon={<IconAlertCircle size={18} />}
      title={displayTitle}
      color="red"
      radius="md"
      className={className}
    >
      <Group justify="space-between" align="center" wrap="wrap">
        <span>{message}</span>
        {onRetry && (
          <Button variant="white" color="red" size="xs" onClick={onRetry}>
            {t('common.retry')}
          </Button>
        )}
      </Group>
    </Alert>
  );
}

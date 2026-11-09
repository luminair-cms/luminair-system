import type { ReactElement } from 'react';
import { Badge, Group, type MantineSize } from '@mantine/core';
import { useTranslation } from 'react-i18next';

// ---------------------------------------------------------------------------
// Types & Interfaces
// ---------------------------------------------------------------------------

export type DocumentLifecycleStatus = 'draft' | 'published' | 'modified' | 'unpublished';

export interface StatusBadgeProps {
  status: DocumentLifecycleStatus;
  revision?: number | null;
  draftVersion?: number | null;
  size?: MantineSize;
}

// ---------------------------------------------------------------------------
// Primary Export
// ---------------------------------------------------------------------------

export function StatusBadge({
  status,
  revision,
  draftVersion,
  size = 'sm',
}: StatusBadgeProps): ReactElement {
  const { t } = useTranslation();
  const color = STATUS_COLOR_MAP[status];
  const label = t(`status.${status}`);

  const hasExtraBadges = revision != null || draftVersion != null;
  if (!hasExtraBadges) {
    return (
      <Badge color={color} size={size} variant="filled">
        {label}
      </Badge>
    );
  }

  return (
    <Group gap="xs" wrap="nowrap" align="center">
      <Badge color={color} size={size} variant="filled">
        {label}
      </Badge>
      {revision != null && <RevisionBadge revision={revision} />}
      {draftVersion != null && <DraftVersionBadge version={draftVersion} />}
    </Group>
  );
}

// ---------------------------------------------------------------------------
// Internal Helpers & Sub-Components
// ---------------------------------------------------------------------------

function RevisionBadge({ revision }: { revision: number }): ReactElement {
  const { t } = useTranslation();
  return (
    <Badge color="gray" size="xs" variant="outline">
      {t('status.revision', { revision, defaultValue: `r${revision}` })}
    </Badge>
  );
}

function DraftVersionBadge({ version }: { version: number }): ReactElement {
  const { t } = useTranslation();
  return (
    <Badge color="orange" size="xs" variant="outline">
      {t('status.draftVersion', { version, defaultValue: `v${version}` })}
    </Badge>
  );
}

// ---------------------------------------------------------------------------
// Constants & Static Metadata
// ---------------------------------------------------------------------------

const STATUS_COLOR_MAP: Record<DocumentLifecycleStatus, string> = {
  draft: 'yellow',
  published: 'green',
  modified: 'blue',
  unpublished: 'gray',
};

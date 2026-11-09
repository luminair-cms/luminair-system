import type { ReactElement } from 'react';
import { Anchor, Breadcrumbs, Text } from '@mantine/core';
import { IconChevronRight } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';

// ---------------------------------------------------------------------------
// Types & Interfaces
// ---------------------------------------------------------------------------

export interface BreadcrumbItem {
  label: string;
  href?: string;
}

export interface BreadcrumbBarProps {
  items: BreadcrumbItem[];
}

// ---------------------------------------------------------------------------
// Primary Export
// ---------------------------------------------------------------------------

export function BreadcrumbBar({ items }: BreadcrumbBarProps): ReactElement {
  return (
    <Breadcrumbs separator={<IconChevronRight size={14} />}>
      {items.map((item, index) => renderBreadcrumbItem(item, index))}
    </Breadcrumbs>
  );
}

// ---------------------------------------------------------------------------
// Internal Helpers
// ---------------------------------------------------------------------------

function renderBreadcrumbItem(item: BreadcrumbItem, index: number): ReactElement {
  if (item.href) {
    return (
      <Anchor key={index} component={Link} to={item.href} size="sm">
        {item.label}
      </Anchor>
    );
  }

  return (
    <Text key={index} size="sm" c="dimmed">
      {item.label}
    </Text>
  );
}

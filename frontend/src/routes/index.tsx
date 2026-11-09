import type { ReactElement } from 'react';
import { Badge, Card, Center, Container, Group, Loader, Stack, Text, Title } from '@mantine/core';
import { createRoute } from '@tanstack/react-router';
import { useSystemConfig } from '@/api/system.api';
import type { SystemConfigDto } from '@/api/types';
import { ErrorAlert } from '@/components/common/ErrorAlert';
import { Route as rootRoute } from './__root';

// ---------------------------------------------------------------------------
// Primary Route Definition
// ---------------------------------------------------------------------------

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: DashboardPage,
});

// ---------------------------------------------------------------------------
// Sub-Components
// ---------------------------------------------------------------------------

function DashboardPage(): ReactElement {
  return (
    <Container size="lg" py="md">
      <Stack gap="lg">
        <div>
          <Title order={2}>Luminair Admin Studio</Title>
          <Text c="dimmed" mt={4}>
            Welcome to the Luminair content administration and schema management console.
          </Text>
        </div>
        <SystemConfigSummary />
      </Stack>
    </Container>
  );
}

function SystemConfigSummary(): ReactElement {
  const { data, isLoading, error, refetch } = useSystemConfig();

  if (isLoading) {
    return (
      <Center py="xl">
        <Loader size="md" />
      </Center>
    );
  }

  if (error) {
    return <ErrorAlert error={error} onRetry={() => void refetch()} />;
  }

  if (!data) {
    return <Text c="dimmed">No system configuration available.</Text>;
  }

  return <ConfigDetailsCard config={data} />;
}

function ConfigDetailsCard({ config }: { config: SystemConfigDto }): ReactElement {
  return (
    <Card withBorder radius="md" p="md">
      <Stack gap="sm">
        <Title order={4}>System Configuration</Title>
        <Group justify="space-between">
          <Text size="sm" c="dimmed">Configuration ID</Text>
          <Text size="sm" fw={500}>{config.id}</Text>
        </Group>
        <Group justify="space-between">
          <Text size="sm" c="dimmed">Default Locale</Text>
          <Badge variant="outline">{config.defaultLocale}</Badge>
        </Group>
        <Group justify="space-between">
          <Text size="sm" c="dimmed">Available Locales</Text>
          <Group gap={4}>
            {config.availableLocales.map((loc) => (
              <Badge key={loc} size="xs" variant="light">{loc}</Badge>
            ))}
          </Group>
        </Group>
      </Stack>
    </Card>
  );
}

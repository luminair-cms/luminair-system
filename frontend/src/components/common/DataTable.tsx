import { type ReactElement, type ReactNode } from 'react';
import { Center, Group, Loader, Pagination, Stack, Table, Text } from '@mantine/core';

// ---------------------------------------------------------------------------
// Types & Interfaces
// ---------------------------------------------------------------------------

export interface DataTableColumn<T> {
  key: string;
  header: string;
  render?: (item: T) => ReactNode;
  width?: string | number;
}

export interface DataTableProps<T> {
  data: T[];
  columns: DataTableColumn<T>[];
  keyExtractor: (item: T) => string | number;
  loading?: boolean;
  emptyMessage?: string;
  page?: number;
  totalPages?: number;
  onPageChange?: (page: number) => void;
}

interface TableHeaderProps<T> {
  columns: DataTableColumn<T>[];
}

interface TableBodyProps<T> {
  data: T[];
  columns: DataTableColumn<T>[];
  keyExtractor: (item: T) => string | number;
  loading?: boolean;
  emptyMessage?: string;
}

interface TableRowProps<T> {
  item: T;
  columns: DataTableColumn<T>[];
}

interface TablePaginationProps {
  page?: number;
  totalPages?: number;
  onPageChange?: (page: number) => void;
}

// ---------------------------------------------------------------------------
// Primary Export (Generic Data Table)
// ---------------------------------------------------------------------------

export function DataTable<T>({
  data,
  columns,
  keyExtractor,
  loading = false,
  emptyMessage = 'No records found',
  page,
  totalPages,
  onPageChange,
}: DataTableProps<T>): ReactElement {
  return (
    <Stack gap="md">
      <Table striped highlightOnHover withTableBorder>
        <DataTableHeader columns={columns} />
        <DataTableBody
          data={data}
          columns={columns}
          keyExtractor={keyExtractor}
          loading={loading}
          emptyMessage={emptyMessage}
        />
      </Table>
      <DataTablePagination page={page} totalPages={totalPages} onPageChange={onPageChange} />
    </Stack>
  );
}

// ---------------------------------------------------------------------------
// Internal Helpers & Sub-Components
// ---------------------------------------------------------------------------

function DataTableHeader<T>({ columns }: TableHeaderProps<T>): ReactElement {
  return (
    <Table.Thead>
      <Table.Tr>
        {columns.map((column) => (
          <Table.Th key={column.key} w={column.width}>
            {column.header}
          </Table.Th>
        ))}
      </Table.Tr>
    </Table.Thead>
  );
}

function DataTableBody<T>({
  data,
  columns,
  keyExtractor,
  loading,
  emptyMessage,
}: TableBodyProps<T>): ReactElement {
  if (loading) {
    return (
      <Table.Tbody>
        <Table.Tr>
          <Table.Td colSpan={columns.length}>
            <Center py="xl">
              <Loader size="md" />
            </Center>
          </Table.Td>
        </Table.Tr>
      </Table.Tbody>
    );
  }

  if (data.length === 0) {
    return (
      <Table.Tbody>
        <Table.Tr>
          <Table.Td colSpan={columns.length}>
            <Center py="xl">
              <Text c="dimmed">{emptyMessage}</Text>
            </Center>
          </Table.Td>
        </Table.Tr>
      </Table.Tbody>
    );
  }

  return (
    <Table.Tbody>
      {data.map((item) => (
        <DataTableRow key={keyExtractor(item)} item={item} columns={columns} />
      ))}
    </Table.Tbody>
  );
}

function DataTableRow<T>({ item, columns }: TableRowProps<T>): ReactElement {
  return (
    <Table.Tr>
      {columns.map((column) => (
        <Table.Td key={column.key}>
          {column.render ? column.render(item) : resolveCellValue(item, column.key)}
        </Table.Td>
      ))}
    </Table.Tr>
  );
}

function DataTablePagination({
  page,
  totalPages,
  onPageChange,
}: TablePaginationProps): ReactElement | null {
  if (!page || !totalPages || totalPages <= 1 || !onPageChange) {
    return null;
  }

  return (
    <Group justify="center">
      <Pagination value={page} onChange={onPageChange} total={totalPages} />
    </Group>
  );
}

function resolveCellValue<T>(item: T, key: string): ReactNode {
  if (typeof item === 'object' && item !== null && key in item) {
    const value = (item as Record<string, unknown>)[key];
    if (value === null || value === undefined) {
      return '';
    }
    return String(value);
  }
  return '';
}

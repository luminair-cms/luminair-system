import { createRequire } from 'node:module';

// TypeScript 7.0 removed the programmatic JS compiler API in favor of the native Go engine.
// Redirect tooling calls for 'typescript' to '@typescript/typescript6' (the official TS 6 API compatibility layer).
const require = createRequire(import.meta.url);
const Module = require('node:module');
const origRequire = Module.prototype.require;
Module.prototype.require = function (id) {
  if (id === 'typescript') {
    return origRequire.call(this, '@typescript/typescript6');
  }
  return origRequire.apply(this, arguments);
};

const js = (await import('@eslint/js')).default;
const tseslint = (await import('typescript-eslint')).default;

export default tseslint.config(
  {
    ignores: ['dist', 'node_modules', '*.config.js'],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    rules: {
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/no-unused-vars': [
        'error',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          caughtErrorsIgnorePattern: '^_',
        },
      ],
    },
  }
);

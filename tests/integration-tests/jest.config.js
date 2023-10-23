module.exports = {
  collectCoverage: true,
  coverageReporters: ['html', 'json-summary'],
  collectCoverageFrom: ['tests/*.ts'],
  testEnvironment: 'node',
  setupFiles: ["<rootDir>/tests/setup-tests.ts"],
  moduleFileExtensions: ['js', 'json', 'jsx', 'ts', 'tsx', 'node', 'd.ts'],
  roots: ['<rootDir>/tests'],
  preset: 'ts-jest',
  testMatch: ['**/?(*.)+(spec).(js|ts|tsx)'],
  testRunner: 'jest-circus/runner',
  cacheDirectory: '<rootDir>/.jest-cache',
};

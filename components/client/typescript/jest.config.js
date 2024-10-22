module.exports = {
  collectCoverageFrom: [
    "src/**/*.ts",
  ],
  coverageProvider: "v8",
  // globalSetup: './tests/setup.ts',
  preset: 'ts-jest',
  rootDir: '',
  testPathIgnorePatterns: [
    "/node_modules/",
    "/dist/"
  ],
  transform: {},
  transformIgnorePatterns: ["./dist/.+\\.js"]
};

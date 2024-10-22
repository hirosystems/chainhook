import * as fs from 'fs';
import * as path from 'path';
import { Interceptable, MockAgent, setGlobalDispatcher } from 'undici';
import { ChainhookEventObserver, EventObserverOptions, EventObserverPredicate } from '../src';
import { recallPersistedPredicatesFromDisk, savePredicateToDisk } from '../src/predicates';

function deletePredicates(dir: string) {
  const files = fs.readdirSync(dir);
  for (const file of files) {
    const filePath = path.join(dir, file);
    const stat = fs.statSync(filePath);
    if (stat.isFile() && file.endsWith('.json')) fs.unlinkSync(filePath);
  }
}

describe('predicates', () => {
  let mockAgent: MockAgent;
  let mockClient: Interceptable;
  let server: ChainhookEventObserver;
  let observer: EventObserverOptions;

  const testPredicate: EventObserverPredicate = {
    name: 'test',
    version: 1,
    chain: 'stacks',
    networks: {
      mainnet: {
        if_this: {
          scope: 'block_height',
          higher_than: 1,
        },
      },
    },
  };

  beforeEach(() => {
    mockAgent = new MockAgent();
    mockAgent.disableNetConnect();
    mockClient = mockAgent.get('http://127.0.0.1:20456');
    mockClient
      .intercept({
        path: '/ping',
        method: 'GET',
      })
      .reply(200);
    setGlobalDispatcher(mockAgent);
    observer = {
      hostname: '0.0.0.0',
      port: 3999,
      auth_token: 'token',
      external_base_url: 'http://myserver.com',
      wait_for_chainhook_node: true,
      validate_chainhook_payloads: false,
      predicate_disk_file_path: './tmp',
      node_type: 'chainhook',
    };
    server = new ChainhookEventObserver(observer, {
      base_url: 'http://127.0.0.1:20456',
    });
    deletePredicates(observer.predicate_disk_file_path);
  });

  afterEach(async () => {
    mockClient
      .intercept({
        path: /\/v1\/chainhooks\/stacks\/(.*)/,
        method: 'DELETE',
      })
      .reply(200);
    await server.close();
    await mockAgent.close();
  });

  test('registers and persists new predicate to disk', async () => {
    mockClient
      .intercept({
        path: '/v1/chainhooks',
        method: 'POST',
      })
      .reply(200);

    expect(fs.existsSync(`${observer.predicate_disk_file_path}/predicate-test.json`)).toBe(false);
    await server.start([testPredicate], async () => {});

    expect(fs.existsSync(`${observer.predicate_disk_file_path}/predicate-test.json`)).toBe(true);
    const disk = recallPersistedPredicatesFromDisk(observer.predicate_disk_file_path);
    const storedPredicate = disk.get('test');
    expect(storedPredicate).not.toBeUndefined();
    expect(storedPredicate?.name).toBe(testPredicate.name);
    expect(storedPredicate?.version).toBe(testPredicate.version);
    expect(storedPredicate?.chain).toBe(testPredicate.chain);
    expect(storedPredicate?.networks.mainnet).toStrictEqual(testPredicate.networks.mainnet);
    expect(storedPredicate?.networks.mainnet?.then_that).toStrictEqual({
      http_post: {
        authorization_header: 'Bearer token',
        url: 'http://myserver.com/payload',
      },
    });
    expect(storedPredicate?.uuid).not.toBeUndefined();

    mockAgent.assertNoPendingInterceptors();
  });

  describe('pre-stored', () => {
    beforeEach(() => {
      savePredicateToDisk(observer.predicate_disk_file_path, {
        uuid: 'e2777d77-473a-4c1d-9012-152deb36bf4c',
        name: 'test',
        version: 1,
        chain: 'stacks',
        networks: {
          mainnet: {
            if_this: {
              scope: 'block_height',
              higher_than: 1,
            },
            then_that: {
              http_post: {
                url: 'http://test',
                authorization_header: 'Bearer x',
              },
            },
          },
        },
      });
      expect(fs.existsSync(`${observer.predicate_disk_file_path}/predicate-test.json`)).toBe(true);
    });

    test('resumes active predicate', async () => {
      mockClient
        .intercept({
          path: '/v1/chainhooks/e2777d77-473a-4c1d-9012-152deb36bf4c',
          method: 'GET',
        })
        .reply(200, { result: { enabled: true, status: { type: 'scanning' } }, status: 200 });

      await server.start([testPredicate], async () => {});

      mockAgent.assertNoPendingInterceptors();
      expect(fs.existsSync(`${observer.predicate_disk_file_path}/predicate-test.json`)).toBe(true);
    });

    test('re-registers dead predicate', async () => {
      mockClient
        .intercept({
          path: '/v1/chainhooks/e2777d77-473a-4c1d-9012-152deb36bf4c',
          method: 'GET',
        })
        .reply(200, { result: { enabled: true, status: { type: 'interrupted' } }, status: 200 });
      mockClient
        .intercept({
          path: '/v1/chainhooks/stacks/e2777d77-473a-4c1d-9012-152deb36bf4c',
          method: 'DELETE',
        })
        .reply(200);
      mockClient
        .intercept({
          path: '/v1/chainhooks',
          method: 'POST',
        })
        .reply(200);

      await server.start([testPredicate], async () => {});

      mockAgent.assertNoPendingInterceptors();
      expect(fs.existsSync(`${observer.predicate_disk_file_path}/predicate-test.json`)).toBe(true);
      const disk = recallPersistedPredicatesFromDisk(observer.predicate_disk_file_path);
      const storedPredicate = disk.get('test');
      // Should have a different uuid
      expect(storedPredicate?.uuid).not.toBe('e2777d77-473a-4c1d-9012-152deb36bf4c');
    });
  });
});

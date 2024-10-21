import * as fs from 'fs';
import * as path from 'path';
import {
  ChainhookNodeOptions,
  CompiledServerPredicateSchema,
  ServerOptions,
  ServerPredicate,
} from './server';
import { logger } from './util/logger';
import { Predicate, ThenThatHttpPost } from './schemas/predicate';
import { request } from 'undici';

function chainhookPredicatePath(
  predicate: ServerPredicate,
  serverOpts: ServerOptions,
  chainhookOpts: ChainhookNodeOptions
): string {
  return serverOpts.node_type === 'chainhook'
    ? `${chainhookOpts.base_url}/v1/chainhooks/${predicate.chain}/${encodeURIComponent(
        predicate.uuid
      )}`
    : `${chainhookOpts.base_url}/v1/observers/${encodeURIComponent(predicate.uuid)}`;
}

function recallPersistedPredicatesFromDisk(basePath: string): ServerPredicate[] {
  const predicates: ServerPredicate[] = [];
  try {
    if (!fs.existsSync(basePath)) return [];
    for (const file of fs.readdirSync(basePath)) {
      if (file.endsWith('.json')) {
        const predicate = fs.readFileSync(path.join(basePath, file), 'utf-8');
        if (CompiledServerPredicateSchema.Check(predicate)) {
          logger.info(`ChainhookEventObserver recalled predicate ${predicate.uuid} from disk`);
          predicates.push(predicate as ServerPredicate);
        }
      }
    }
  } catch (error) {
    logger.error(error, `ChainhookEventObserver unable to retrieve persisted predicates from disk`);
    return [];
  }
  return predicates;
}

function persistPredicateToDisk(basePath: string, predicate: ServerPredicate) {
  const predicatePath = `${basePath}/predicate-${encodeURIComponent(predicate.uuid)}.json`;
  try {
    fs.mkdirSync(basePath, { recursive: true });
    fs.writeFileSync(predicatePath, JSON.stringify(predicate, null, 2));
    logger.info(`ChainhookEventObserver persisted predicate ${predicate.uuid} to disk`);
  } catch (error) {
    logger.error(
      error,
      `ChainhookEventObserver unable to persist predicate ${predicate.uuid} to disk`
    );
  }
}

function removePredicateFromDisk(basePath: string, predicate: ServerPredicate) {
  const predicatePath = `${basePath}/predicate-${encodeURIComponent(predicate.uuid)}.json`;
  if (fs.existsSync(predicatePath)) fs.rmSync(predicatePath);
}

/** Registers predicates with the Chainhook server */
export async function registerPredicates(
  predicates: ServerPredicate[],
  serverOpts: ServerOptions,
  chainhookOpts: ChainhookNodeOptions
): Promise<void> {
  let predicatesToRegister = predicates;
  if (serverOpts.predicates_disk_file_path) {
    logger.info(`ChainhookEventObserver recalling predicates from disk`);
    predicatesToRegister = recallPersistedPredicatesFromDisk(serverOpts.predicates_disk_file_path);
  }
  if (predicatesToRegister.length === 0) {
    logger.info(`ChainhookEventObserver does not have predicates to register`);
    return;
  }
  const nodeType = serverOpts.node_type ?? 'chainhook';
  const path = nodeType === 'chainhook' ? `/v1/chainhooks` : `/v1/observers`;
  const registerUrl = `${chainhookOpts.base_url}${path}`;
  logger.info(
    predicatesToRegister,
    `ChainhookEventObserver registering predicates at ${registerUrl}`
  );
  for (const predicate of predicatesToRegister) {
    const thenThat: ThenThatHttpPost = {
      http_post: {
        url: `${serverOpts.external_base_url}/payload`,
        authorization_header: `Bearer ${serverOpts.auth_token}`,
      },
    };
    try {
      const body = predicate as Predicate;
      if ('mainnet' in body.networks) body.networks.mainnet.then_that = thenThat;
      if ('testnet' in body.networks) body.networks.testnet.then_that = thenThat;
      await request(registerUrl, {
        method: 'POST',
        body: JSON.stringify(body),
        headers: { 'content-type': 'application/json' },
        throwOnError: true,
      });
      logger.info(
        `ChainhookEventObserver registered '${predicate.name}' predicate (${predicate.uuid})`
      );
      if (serverOpts.predicates_disk_file_path)
        persistPredicateToDisk(serverOpts.predicates_disk_file_path, predicate);
    } catch (error) {
      logger.error(error, `ChainhookEventObserver unable to register predicate`);
    }
  }
}

/** Removes predicates from the Chainhook server */
export async function removePredicates(
  predicates: ServerPredicate[],
  serverOpts: ServerOptions,
  chainhookOpts: ChainhookNodeOptions
): Promise<void> {
  if (predicates.length === 0) {
    logger.info(`ChainhookEventObserver does not have predicates to close`);
    return;
  }
  logger.info(`ChainhookEventObserver closing predicates at ${chainhookOpts.base_url}`);
  const removals = predicates.map(
    predicate =>
      new Promise<void>((resolve, reject) => {
        request(chainhookPredicatePath(predicate, serverOpts, chainhookOpts), {
          method: 'DELETE',
          headers: { 'content-type': 'application/json' },
          throwOnError: true,
        })
          .then(() => {
            logger.info(
              `ChainhookEventObserver removed '${predicate.name}' predicate (${predicate.uuid})`
            );
            if (serverOpts.predicates_disk_file_path)
              removePredicateFromDisk(serverOpts.predicates_disk_file_path, predicate);
            resolve();
          })
          .catch(error => {
            logger.error(error, `ChainhookEventObserver unable to deregister predicate`);
            reject(error);
          });
      })
  );
  await Promise.allSettled(removals);
}

export async function predicateHealthCheck(
  predicates: ServerPredicate[],
  serverOpts: ServerOptions,
  chainhookOpts: ChainhookNodeOptions
): Promise<void> {
  for (const predicate of predicates) {
    const result = await request(chainhookPredicatePath(predicate, serverOpts, chainhookOpts), {
      method: 'GET',
      headers: { 'content-type': 'application/json' },
      throwOnError: true,
    });
    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
    const json = await result.body.json();
    if (!['streaming', 'scanning'].includes(json.result.status.type)) {
      //
    }
  }
}

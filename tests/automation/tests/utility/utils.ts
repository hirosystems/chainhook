export const wait = async (ms: number) => {
  return new Promise((resolve) => {
    setTimeout(() => {
      resolve(true);
    }, ms);
  });
};

export function classSelector<T extends string>(name: T): string {
  return `.${name}`;
}

export function multiClassSelector<T extends string>(name: T): string {
  return `.${name.split(' ').join('.')}`;
}

export function idSelector<T extends string>(name: T): string {
  return `[id="${name}"]`;
}

export function textSelector<T extends string>(name: T): string {
  return `text="${name}"`;
}
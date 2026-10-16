const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

function assertDeclaredMethods(addon) {
  const source = fs.readFileSync(path.join(__dirname, '../ohos/index.d.ts'), 'utf8');
  for (const name of ['OhEngine', 'OhStartupLifecycle']) {
    const body = source.match(new RegExp(`export interface ${name} \\{([\\s\\S]*?)\\n\\}`))[1];
    const declared = [...body.matchAll(/^\s+(\w+)\(/gm)].map(match => match[1]).sort();
    const actual = Object.getOwnPropertyNames(addon[name].prototype).filter(method => method !== 'constructor').sort();
    assert.deepEqual(declared, actual, `${name} declarations must match the loaded native exports`);
  }
}

module.exports = { assertDeclaredMethods };

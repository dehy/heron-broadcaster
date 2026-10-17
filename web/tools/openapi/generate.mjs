// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Generates web/src/api/schema.ts from web/openapi.json, the OpenAPI document
// of heron-api (`heron-api openapi > web/openapi.json`).

import { readFile, writeFile } from "node:fs/promises";
import openapiTS, { astToString } from "openapi-typescript";

const web = new URL("../../", import.meta.url);
const document = JSON.parse(await readFile(new URL("openapi.json", web), "utf8"));
const types = astToString(await openapiTS(document));
const header = `// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Generated from openapi.json by \`npm run generate-api\`. Do not edit.

`;
await writeFile(new URL("src/api/schema.ts", web), header + types);

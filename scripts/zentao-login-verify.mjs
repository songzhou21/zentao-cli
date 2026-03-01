#!/usr/bin/env node

import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

function parseArgs(argv) {
  const args = {};
  for (let i = 2; i < argv.length; i += 1) {
    const key = argv[i];
    const val = argv[i + 1];
    if (!key.startsWith("--")) continue;
    if (val && !val.startsWith("--")) {
      args[key.slice(2)] = val;
      i += 1;
    } else {
      args[key.slice(2)] = "true";
    }
  }
  return args;
}

function md5(input) {
  return createHash("md5").update(input).digest("hex");
}

function runCurl(args, env, phase) {
  const maxAttempts = 3;
  let lastErr = "";
  const proxy = argsObj.proxy || process.env.ZENTAO_PROXY || process.env.http_proxy || process.env.HTTP_PROXY || "";
  const baseArgs = [
    "--http1.1",
    "-A",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36",
  ];
  if (proxy) {
    baseArgs.push("--proxy", proxy);
  }

  for (let i = 1; i <= maxAttempts; i += 1) {
    const res = spawnSync("curl", [...baseArgs, ...args], {
      env,
      encoding: "utf8",
    });
    if (res.status === 0) return;
    lastErr = (res.stderr || res.stdout || "").trim();
    if (!/curl:\s*\(52\)/.test(lastErr) || i === maxAttempts) {
      throw new Error(`${phase}: curl failed(${res.status}): ${lastErr}`);
    }
  }
  throw new Error(`${phase}: curl failed: ${lastErr}`);
}

function readText(path) {
  return readFileSync(path, "utf8");
}

function parseVerifyRand(loginHtml) {
  const m = loginHtml.match(/name=['"]verifyRand['"][^>]*value=['"](\d+)['"]/i);
  if (!m) throw new Error("verifyRand not found in login page");
  return m[1];
}

function parseSetCookies(rawHeaders) {
  return rawHeaders
    .split(/\r?\n/)
    .filter((line) => /^Set-Cookie:/i.test(line))
    .map((line) => line.replace(/^Set-Cookie:\s*/i, "").trim());
}

function extractCookieFromJar(jarText, name) {
  const rows = jarText.split(/\r?\n/);
  for (const row of rows) {
    if (!row) continue;
    if (row.startsWith("#") && !row.startsWith("#HttpOnly_")) continue;
    const normalized = row.startsWith("#HttpOnly_") ? row.slice(1) : row;
    const cols = normalized.split("\t");
    if (cols.length >= 7 && cols[5] === name) return cols[6];
  }
  return "";
}

const argsObj = parseArgs(process.argv);
const baseUrl = (argsObj["base-url"] || process.env.ZENTAO_BASE_URL || "").replace(/\/+$/, "");
const account = argsObj.account || process.env.ZENTAO_ACCOUNT || "";
const password = argsObj.password || process.env.ZENTAO_PASSWORD || "";

if (!baseUrl || !account || !password) {
  console.error("Usage:");
  console.error(
    "  node scripts/zentao-login-verify.mjs --base-url http://shendao.sharexm.cn/zentao --account <user> --password <pass>"
  );
  process.exit(1);
}

const tmp = mkdtempSync(join(tmpdir(), "zentao-login-"));
const jar = join(tmp, "cookie.jar");
const loginPage = join(tmp, "login.html");
const loginHeaders = join(tmp, "login.headers");
const postBody = join(tmp, "post.body");
const postHeaders = join(tmp, "post.headers");
const myBody = join(tmp, "my.html");
const myHeaders = join(tmp, "my.headers");
const env = { ...process.env };
delete env.ALL_PROXY;
delete env.all_proxy;
delete env.HTTPS_PROXY;
delete env.https_proxy;
delete env.HTTP_PROXY;
delete env.http_proxy;

try {
  const loginUrl = `${baseUrl}/user-login-L3plbnRhby8=.html`;
  runCurl(
    [
      "-sS",
      "--max-time",
      "30",
      "-c",
      jar,
      "-b",
      jar,
      "-D",
      loginHeaders,
      "-o",
      loginPage,
      loginUrl,
    ],
    env,
    "GET login page"
  );

  const html = readText(loginPage);
  const verifyRand = parseVerifyRand(html);
  const passHash = md5(md5(password) + verifyRand);

  const form = new URLSearchParams({
    account,
    password: passHash,
    passwordStrength: "2",
    referer: "/zentao/",
    verifyRand,
    keepLogin: "1",
    "keepLogin[]": "on",
  });

  runCurl(
    [
      "-sS",
      "--max-time",
      "30",
      "-c",
      jar,
      "-b",
      jar,
      "-D",
      postHeaders,
      "-o",
      postBody,
      "-H",
      "Accept: application/json, text/javascript, */*; q=0.01",
      "-H",
      "Content-Type: application/x-www-form-urlencoded; charset=UTF-8",
      "-H",
      "X-Requested-With: XMLHttpRequest",
      "-H",
      `Origin: ${new URL(baseUrl).origin}`,
      "-H",
      `Referer: ${baseUrl}/user-login.html`,
      "--data-raw",
      form.toString(),
      `${baseUrl}/user-login.html`,
    ],
    env,
    "POST login form"
  );

  runCurl(
    ["-sS", "--max-time", "30", "-c", jar, "-b", jar, "-D", myHeaders, "-o", myBody, `${baseUrl}/my/`],
    env,
    "GET /my/"
  );

  const postRespBody = readText(postBody);
  const loginRespHeaders = readText(loginHeaders);
  const postRespHeaders = readText(postHeaders);
  const myRespHeaders = readText(myHeaders);
  const jarText = readText(jar);
  const myHtml = readText(myBody);
  const setCookiesByUrl = [
    { url: loginUrl, cookies: parseSetCookies(loginRespHeaders) },
    { url: `${baseUrl}/user-login.html`, cookies: parseSetCookies(postRespHeaders) },
    { url: `${baseUrl}/my/`, cookies: parseSetCookies(myRespHeaders) },
  ];

  const za = extractCookieFromJar(jarText, "za");
  const zp = extractCookieFromJar(jarText, "zp");
  const zentaosid = extractCookieFromJar(jarText, "zentaosid");
  const loggedIn = !/user-login-/i.test(myHtml) && /我的地盘|退出|my-profile/i.test(myHtml);

  console.log(`verifyRand: ${verifyRand}`);
  console.log(`login response body: ${postRespBody.trim()}`);
  console.log("set-cookie by url:");
  for (const entry of setCookiesByUrl) {
    console.log(`  ${entry.url}`);
    if (entry.cookies.length === 0) {
      console.log("    (none)");
      continue;
    }
    for (const line of entry.cookies) {
      const safe = line
        .replace(/(zp=)[^;]+/i, "$1***")
        .replace(/(zentaosid=)[^;]+/i, "$1***");
      console.log(`    ${safe}`);
    }
  }
  console.log(`cookie jar zentaosid: ${zentaosid ? "***" : "(missing)"}`);
  console.log(`cookie jar za: ${za || "(missing)"}`);
  console.log(`cookie jar zp: ${zp ? "***" : "(missing)"}`);
  console.log(`login state via /my/: ${loggedIn ? "logged-in" : "not-logged-in"}`);
} finally {
  rmSync(tmp, { recursive: true, force: true });
}

export const GIT_USERNAME = "toki-agent";

export const basicAuthPassword = (gitAuthHeader: string): string | undefined => {
  const match = /^Basic\s+(.+)$/i.exec(gitAuthHeader.trim());

  if (match === null) {
    return undefined;
  }

  const decoded = Buffer.from(match[1], "base64").toString("utf8");
  const separatorIndex = decoded.indexOf(":");

  if (separatorIndex === -1) {
    return undefined;
  }

  return decoded.slice(separatorIndex + 1);
};

export const gitUrlWithUsername = (value: string, username: string): string | undefined => {
  try {
    const url = new URL(value);

    if (url.protocol !== "https:") {
      return undefined;
    }

    url.username = username;
    url.password = "";
    return url.toString();
  } catch {
    return undefined;
  }
};

export const gitAskPassScript = (password: string): string =>
  [
    "#!/bin/sh",
    'case "$1" in',
    `  *Username*) printf '%s\\n' ${shellQuote(GIT_USERNAME)} ;;`,
    `  *Password*) printf '%s\\n' ${shellQuote(password)} ;;`,
    "  *) printf '\\n' ;;",
    "esac",
    "",
  ].join("\n");

export const gitCredentialStoreLine = (password: string): string => {
  const url = new URL("https://dev.azure.com");
  url.username = GIT_USERNAME;
  url.password = password;
  return url.toString();
};

const shellQuote = (value: string): string => `'${value.replaceAll("'", "'\\''")}'`;

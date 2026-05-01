const TOKEN_PATTERNS: ReadonlyArray<RegExp> = [
  /(?:authorization:\s*bearer\s+)[^\s"'`]+/gi,
  /(?:authorization:\s*basic\s+)[^\s"'`]+/gi,
  /(?:token|api[_-]?key|password|secret)=([^&\s]+)/gi,
  /https:\/\/([^:\s/@]+):([^@\s]+)@/gi,
];

export function redactLog(input: string): string {
  return TOKEN_PATTERNS.reduce((current, pattern) => {
    if (pattern.source.startsWith("https")) {
      return current.replace(pattern, "https://$1:[REDACTED]@");
    }

    return current.replace(pattern, (match) => {
      const separator = match.includes("=") ? "=" : " ";
      const [prefix] = match.split(separator);
      return `${prefix}${separator}[REDACTED]`;
    });
  }, input);
}

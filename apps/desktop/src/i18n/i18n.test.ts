import { afterEach, describe, expect, it } from "vitest";
import {
  catalogs,
  detectLocale,
  formatDateTime,
  formatDecimal,
  formatTokens,
  isExplicitSimplifiedChinese,
  translate,
} from "./index";

describe("first-launch locale detection", () => {
  it("selects zh-CN only for explicitly Simplified Chinese locales", () => {
    expect(isExplicitSimplifiedChinese("zh-CN")).toBe(true);
    expect(isExplicitSimplifiedChinese("zh-SG")).toBe(true);
    expect(isExplicitSimplifiedChinese("zh-Hans")).toBe(true);
    expect(isExplicitSimplifiedChinese("zh-Hans-CN")).toBe(true);
    expect(isExplicitSimplifiedChinese("zh")).toBe(false);
    expect(isExplicitSimplifiedChinese("zh-TW")).toBe(false);
    expect(isExplicitSimplifiedChinese("zh-HK")).toBe(false);
    expect(isExplicitSimplifiedChinese("zh-MO")).toBe(false);
    expect(isExplicitSimplifiedChinese("zh-Hant")).toBe(false);
    expect(isExplicitSimplifiedChinese("zh-Hant-CN")).toBe(false);
    expect(isExplicitSimplifiedChinese("en-CN")).toBe(false);
  });

  it("detects zh-CN from a Simplified Chinese system locale", () => {
    expect(detectLocale(["zh-CN", "en-US"])).toBe("zh-CN");
    expect(detectLocale(["zh-Hans-CN"])).toBe("zh-CN");
  });

  it("falls back to en for other or ambiguous locales", () => {
    expect(detectLocale(["en-US"])).toBe("en");
    expect(detectLocale(["fr-FR", "en-US"])).toBe("en");
    expect(detectLocale(["zh"])).toBe("en");
    expect(detectLocale(["zh-TW", "zh-CN"])).toBe("en");
    expect(detectLocale([])).toBe("en");
  });
});

describe("translation fallback", () => {
  afterEach(() => {
    // Restore any key removed to exercise the English fallback.
    for (const [key, value] of Object.entries(catalogs.en)) {
      if (!(key in catalogs["zh-CN"])) {
        catalogs["zh-CN"][key as keyof typeof catalogs["zh-CN"]] = value!;
      }
    }
  });

  it("returns the zh-CN entry when present", () => {
    expect(translate("zh-CN", "nav.overview")).toBe("概览");
  });

  it("falls back to English when the runtime translation is missing", () => {
    delete catalogs["zh-CN"]["settings.heading"];
    expect(catalogs["zh-CN"]["settings.heading"]).toBeUndefined();
    expect(translate("zh-CN", "settings.heading")).toBe(catalogs.en["settings.heading"]);
  });

  it("interpolates named parameters", () => {
    expect(translate("en", "a11y.langSelected", { language: "English" })).toBe(
      "Interface language changed to English",
    );
    expect(translate("zh-CN", "a11y.langSelected", { language: "简体中文" })).toBe(
      "界面语言已切换为 简体中文",
    );
  });
});

describe("locale-aware display formatting", () => {
  it("groups token counts exactly without Number rounding", () => {
    expect(formatTokens("123456789012345678", "en")).toBe("123,456,789,012,345,678");
    expect(formatTokens("123456789012345678", "zh-CN")).toBe("123,456,789,012,345,678");
  });

  it("passes non-numeric contract values through untouched", () => {
    expect(formatTokens("unknown", "en")).toBe("unknown");
  });

  it("keeps every fractional digit of money amounts", () => {
    expect(formatDecimal("1234.500", "en")).toBe("1,234.500");
    expect(formatDecimal("-98765.4321", "zh-CN")).toBe("-98,765.4321");
    expect(formatDecimal("42", "en")).toBe("42");
    expect(formatDecimal("n/a", "en")).toBe("n/a");
  });

  it("formats the same instant in the independent reporting zone", () => {
    const instant = new Date("2026-10-18T12:00:00Z");
    expect(formatDateTime(instant, "en", "UTC")).toContain("12:00");
    expect(formatDateTime(instant, "en", "America/New_York")).toContain("8:00");
    expect(formatDateTime(instant, "zh-CN", "UTC")).toContain("12:00");
  });
});

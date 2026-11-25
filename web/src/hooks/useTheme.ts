import { useCallback, useEffect, useState } from 'react';

export type ThemeMode = 'spotify' | 'classic';

const STORAGE_KEY = 'bside-theme';
const THEME_EVENT = 'bside-theme-change';

function readInitialTheme(): ThemeMode {
  if (typeof document !== 'undefined') {
    const attr = document.documentElement.dataset.theme;
    if (attr === 'classic' || attr === 'spotify') {
      return attr;
    }
  }
  if (typeof window !== 'undefined') {
    try {
      const saved = window.localStorage.getItem(STORAGE_KEY);
      if (saved === 'classic' || saved === 'spotify') {
        return saved;
      }
    } catch {
      // ignore storage errors
    }
  }
  return 'spotify';
}

function applyTheme(next: ThemeMode) {
  if (typeof document !== 'undefined') {
    document.documentElement.dataset.theme = next;
  }
  if (typeof window !== 'undefined') {
    try {
      window.localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // ignore storage errors
    }
  }
}

export function useTheme(): {
  theme: ThemeMode;
  setTheme: (t: ThemeMode) => void;
  toggleTheme: () => void;
} {
  const [theme, setThemeState] = useState<ThemeMode>(readInitialTheme);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  useEffect(() => {
    const onThemeEvent = (event: Event) => {
      const detail = (event as CustomEvent<ThemeMode>).detail;
      if (detail === 'spotify' || detail === 'classic') {
        setThemeState(detail);
      }
    };

    const onStorage = (event: StorageEvent) => {
      if (event.key === STORAGE_KEY) {
        const next: ThemeMode = event.newValue === 'classic' ? 'classic' : 'spotify';
        setThemeState(next);
        applyTheme(next);
      }
    };

    window.addEventListener(THEME_EVENT, onThemeEvent);
    window.addEventListener('storage', onStorage);
    return () => {
      window.removeEventListener(THEME_EVENT, onThemeEvent);
      window.removeEventListener('storage', onStorage);
    };
  }, []);

  const setTheme = useCallback((next: ThemeMode) => {
    applyTheme(next);
    setThemeState(next);
    window.dispatchEvent(new CustomEvent<ThemeMode>(THEME_EVENT, { detail: next }));
  }, []);

  const toggleTheme = useCallback(() => {
    setTheme(theme === 'spotify' ? 'classic' : 'spotify');
  }, [theme, setTheme]);

  return { theme, setTheme, toggleTheme };
}

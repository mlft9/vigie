import { isOpenableUrl, isOpenGesture, MAX_URL_LENGTH } from './links';

describe('politique des liens du terminal', () => {
  it.each(['https://example.com', 'http://192.168.1.10:8080/admin', 'https://a.b/c?d=e#f'])(
    'autorise %s',
    (url) => {
      expect(isOpenableUrl(url)).toBe(true);
    },
  );

  it.each([
    'file:///etc/passwd',
    'javascript:alert(1)',
    'ms-settings:',
    'ssh://host',
    'ftp://example.com',
    'https://user:pass@example.com',
    'https://user@example.com',
    'https://exa\nmple.com',
    'example.com',
    '',
    `https://example.com/${'a'.repeat(MAX_URL_LENGTH)}`,
  ])('refuse %j', (url) => {
    expect(isOpenableUrl(url)).toBe(false);
  });

  it('exige Ctrl+clic', () => {
    expect(isOpenGesture({ ctrlKey: false, metaKey: false })).toBe(false);
    expect(isOpenGesture({ ctrlKey: true, metaKey: false })).toBe(true);
  });
});

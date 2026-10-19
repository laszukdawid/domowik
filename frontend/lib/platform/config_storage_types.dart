/// One persisted preference document.
abstract interface class ConfigStorage {
  /// The stored document's UTF-8 bytes, or null when nothing is stored. More
  /// than [limit] bytes means the document is too large; a reader returns at
  /// most `limit + 1` so that is detectable without reading it all.
  Future<List<int>?> read(int limit);

  /// Replaces the stored document.
  Future<void> write(String text);
}

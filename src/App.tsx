import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Search, Clock, Tag, FileText, Copy, X, Filter } from "lucide-react";
import "./App.css";

interface ClipItem {
  id: string;
  content: string;
  summary: string;
  tags: string[];
  timestamp: string;
  source?: string;
}

function App() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<ClipItem[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeTags, setActiveTags] = useState<string[]>([]);
  const [showFilters, setShowFilters] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // Load initial clips
  useEffect(() => {
    inputRef.current?.focus();
    loadRecentClips();
  }, []);

  const loadRecentClips = async () => {
    try {
      setLoading(true);
      setError(null);
      const clips = await invoke<ClipItem[]>("get_recent_clips");
      setResults(clips);
      setSelectedIndex(0);
    } catch (err) {
      setError("Failed to load clips: " + String(err));
      console.error("Failed to load clips:", err);
    } finally {
      setLoading(false);
    }
  };

  const searchClips = async (searchQuery: string) => {
    try {
      setLoading(true);
      setError(null);
      const clips = await invoke<ClipItem[]>("search_clips", { query: searchQuery });
      setResults(clips);
      setSelectedIndex(0);
    } catch (err) {
      setError("Failed to search clips: " + String(err));
      console.error("Failed to search clips:", err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    const timeoutId = setTimeout(() => {
      if (query.trim()) {
        searchClips(query);
      } else {
        loadRecentClips();
      }
    }, 300); // Debounce search

    return () => clearTimeout(timeoutId);
  }, [query]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex(prev => Math.min(prev + 1, results.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex(prev => Math.max(prev - 1, 0));
    } else if (e.key === "Enter" && results[selectedIndex]) {
      handleSelectItem(results[selectedIndex]);
    } else if (e.key === "Escape") {
      if (showFilters) {
        setShowFilters(false);
      } else {
        invoke("hide_window");
      }
    }
  };

  const handleSelectItem = async (item: ClipItem) => {
    try {
      await navigator.clipboard.writeText(item.content);
      await invoke("hide_window");
    } catch (error) {
      console.error("Failed to copy to clipboard:", error);
    }
  };

  const formatTimestamp = (timestamp: string) => {
    const date = new Date(timestamp);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return "Just now";
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    return `${diffDays}d ago`;
  };

  const toggleTag = (tag: string) => {
    setActiveTags(prev => 
      prev.includes(tag) 
        ? prev.filter(t => t !== tag)
        : [...prev, tag]
    );
  };

  const clearQuery = () => {
    setQuery("");
    inputRef.current?.focus();
  };

  const allTags = Array.from(new Set(results.flatMap(item => item.tags)));

  return (
    <div className="w-full h-screen flex items-start justify-center pt-20 px-4">
      <div className="search-container w-full max-w-2xl">
        <div className="flex items-center px-2">
          <Search className="w-5 h-5 text-gray-400 ml-4" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Search your clipboard history..."
            className="search-input"
          />
          {query && (
            <button
              onClick={clearQuery}
              className="p-2 text-gray-400 hover:text-gray-600 transition-colors"
              title="Clear search"
            >
              <X className="w-4 h-4" />
            </button>
          )}
          <button
            onClick={() => setShowFilters(!showFilters)}
            className={`p-2 transition-colors ${showFilters ? 'text-blue-500' : 'text-gray-400 hover:text-gray-600'}`}
            title="Toggle filters"
          >
            <Filter className="w-4 h-4" />
          </button>
        </div>

        {showFilters && (
          <div className="px-6 py-3 border-t border-gray-100/50">
            <div className="flex flex-wrap gap-2">
              {allTags.map(tag => (
                <button
                  key={tag}
                  onClick={() => toggleTag(tag)}
                  className={`tag ${activeTags.includes(tag) ? 'bg-blue-500 text-white' : ''}`}
                >
                  {tag}
                </button>
              ))}
            </div>
          </div>
        )}

        {error && (
          <div className="px-6 py-4 text-red-600 text-sm">
            {error}
          </div>
        )}

        {loading && (
          <div className="px-6 py-4 text-gray-500 text-sm loading-dots">
            Loading
          </div>
        )}

        {!loading && !error && results.length === 0 && (
          <div className="px-6 py-8 text-center text-gray-500">
            {query.trim() ? "No clips found matching your search." : "No clips yet. Start copying text to build your clipboard history!"}
          </div>
        )}

        {!loading && !error && results.length > 0 && (
          <div className="max-h-96 overflow-y-auto">
            {results.map((item, index) => (
              <div
                key={item.id}
                className={`result-item ${index === selectedIndex ? 'selected' : ''}`}
                onClick={() => handleSelectItem(item)}
              >
                <div className="flex items-start justify-between">
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      <FileText className="w-4 h-4 text-gray-400 flex-shrink-0" />
                      <p className="text-sm font-medium text-gray-900 truncate">
                        {item.summary}
                      </p>
                    </div>
                    <p className="text-sm text-gray-600 line-clamp-2 mb-2">
                      {item.content.length > 120 
                        ? `${item.content.substring(0, 120)}...` 
                        : item.content
                      }
                    </p>
                    <div className="flex items-center gap-3 text-xs text-gray-500">
                      <div className="flex items-center gap-1">
                        <Clock className="w-3 h-3" />
                        {formatTimestamp(item.timestamp)}
                      </div>
                      {item.source && (
                        <span>from {item.source}</span>
                      )}
                      {item.tags.length > 0 && (
                        <div className="flex items-center gap-1">
                          <Tag className="w-3 h-3" />
                          <div className="flex gap-1">
                            {item.tags.map(tag => (
                              <span key={tag} className="tag">
                                {tag}
                              </span>
                            ))}
                          </div>
                        </div>
                      )}
                    </div>
                  </div>
                  <Copy className="w-4 h-4 text-gray-400 ml-4 flex-shrink-0" />
                </div>
              </div>
            ))}
          </div>
        )}

        <div className="px-6 py-3 border-t border-gray-100/50 bg-gray-50/30">
          <div className="flex items-center justify-between text-xs text-gray-500">
            <div className="flex items-center gap-4">
              <span>↑↓ Navigate</span>
              <span>↵ Copy</span>
              <span>⎋ Close</span>
              {showFilters && <span>⌘F Filter</span>}
            </div>
            <span>{results.length} clips</span>
          </div>
        </div>
      </div>
    </div>
  );
}

export default App;

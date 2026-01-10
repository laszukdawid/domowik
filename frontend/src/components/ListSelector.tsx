import { useState, useRef, useEffect } from 'react';
import type { CustomList } from '../types';

interface ListSelectorProps {
  customLists: CustomList[];
  selectedListId: number | null;
  onSelect: (listId: number | null) => void;
  onCreateList: () => void;
  onRenameList: (id: number, name: string) => void;
  onDeleteList: (id: number) => void;
}

export default function ListSelector({
  customLists,
  selectedListId,
  onSelect,
  onCreateList,
  onRenameList,
  onDeleteList,
}: ListSelectorProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [filter, setFilter] = useState('');
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editingName, setEditingName] = useState('');
  const dropdownRef = useRef<HTMLDivElement>(null);

  // Close dropdown when clicking outside
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsOpen(false);
        setEditingId(null);
      }
    }
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const selectedName = selectedListId === null
    ? 'All Listings'
    : customLists.find(l => l.id === selectedListId)?.name ?? 'All Listings';

  const filteredLists = customLists.filter(list =>
    list.name.toLowerCase().includes(filter.toLowerCase())
  );

  const handleStartEdit = (e: React.MouseEvent, list: CustomList) => {
    e.stopPropagation();
    setEditingId(list.id);
    setEditingName(list.name);
  };

  const handleSaveEdit = (e: React.FormEvent) => {
    e.preventDefault();
    if (editingId && editingName.trim()) {
      onRenameList(editingId, editingName.trim());
      setEditingId(null);
    }
  };

  const handleDelete = (e: React.MouseEvent, id: number) => {
    e.stopPropagation();
    if (confirm('Delete this list? Listings will not be deleted.')) {
      onDeleteList(id);
      if (selectedListId === id) {
        onSelect(null);
      }
    }
  };

  return (
    <div className="relative" ref={dropdownRef}>
      <div className="flex items-center gap-2">
        <button
          onClick={() => setIsOpen(!isOpen)}
          className="flex items-center gap-2 px-3 py-1.5 bg-white border rounded-md hover:bg-gray-50 text-sm font-medium"
        >
          <span className="truncate max-w-[150px]">{selectedName}</span>
          <svg className="w-4 h-4 text-gray-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
          </svg>
        </button>
        <button
          onClick={onCreateList}
          className="px-2 py-1.5 text-sm bg-blue-500 text-white rounded-md hover:bg-blue-600"
          title="Create new list"
        >
          + New
        </button>
      </div>

      {isOpen && (
        <div className="absolute top-full left-0 mt-1 w-72 bg-white border rounded-lg shadow-lg z-50">
          {/* Filter input */}
          <div className="p-2 border-b">
            <input
              type="text"
              placeholder="Filter lists..."
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              className="w-full px-2 py-1 text-sm border rounded focus:outline-none focus:ring-1 focus:ring-blue-500"
              autoFocus
            />
          </div>

          {/* List options */}
          <div className="max-h-64 overflow-y-auto">
            {/* All Listings option */}
            <button
              onClick={() => {
                onSelect(null);
                setIsOpen(false);
              }}
              className={`w-full px-3 py-2 text-left text-sm hover:bg-gray-50 flex items-center justify-between ${
                selectedListId === null ? 'bg-blue-50 text-blue-700' : ''
              }`}
            >
              <span>All Listings</span>
              {selectedListId === null && (
                <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                  <path fillRule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clipRule="evenodd" />
                </svg>
              )}
            </button>

            {/* Custom lists */}
            {filteredLists.map((list) => (
              <div
                key={list.id}
                className={`px-3 py-2 hover:bg-gray-50 ${
                  selectedListId === list.id ? 'bg-blue-50' : ''
                }`}
              >
                {editingId === list.id ? (
                  <form onSubmit={handleSaveEdit} className="flex items-center gap-2">
                    <input
                      type="text"
                      value={editingName}
                      onChange={(e) => setEditingName(e.target.value)}
                      className="flex-1 px-2 py-0.5 text-sm border rounded focus:outline-none focus:ring-1 focus:ring-blue-500"
                      autoFocus
                      onKeyDown={(e) => {
                        if (e.key === 'Escape') {
                          setEditingId(null);
                        }
                      }}
                    />
                    <button type="submit" className="text-green-600 hover:text-green-700">
                      <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                      </svg>
                    </button>
                  </form>
                ) : (
                  <button
                    onClick={() => {
                      onSelect(list.id);
                      setIsOpen(false);
                    }}
                    className="w-full flex items-center justify-between text-sm"
                  >
                    <span className={`truncate ${selectedListId === list.id ? 'text-blue-700 font-medium' : ''}`}>
                      {list.name}
                      <span className="text-gray-400 ml-1">({list.count})</span>
                    </span>
                    <div className="flex items-center gap-1">
                      {selectedListId === list.id && (
                        <svg className="w-4 h-4 text-blue-700" fill="currentColor" viewBox="0 0 20 20">
                          <path fillRule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clipRule="evenodd" />
                        </svg>
                      )}
                      <button
                        onClick={(e) => handleStartEdit(e, list)}
                        className="p-1 text-gray-400 hover:text-gray-600"
                        title="Rename"
                      >
                        <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
                        </svg>
                      </button>
                      <button
                        onClick={(e) => handleDelete(e, list.id)}
                        className="p-1 text-gray-400 hover:text-red-600"
                        title="Delete"
                      >
                        <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                        </svg>
                      </button>
                    </div>
                  </button>
                )}
              </div>
            ))}

            {filteredLists.length === 0 && filter && (
              <div className="px-3 py-2 text-sm text-gray-500">
                No lists match "{filter}"
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

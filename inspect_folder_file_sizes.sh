#!/bin/bash

# --- Configuration ---
MAX_WORDS=1000000
OUTPUT_FILENAME="processed_folder_content_$(date +%Y%m%d_%H%M%S).txt"
TEMP_SIZE_FILE=$(mktemp)

# Paths/Folders to exclude - added | as a separator for regex
EXCLUDE_REGEX="\.git|\.engram|\.claude|\.idea|target|\.next|node_modules|dist|workers|__tests__|__mocks__|constants|features"

# Specific full filenames to exclude
EXCLUDE_FILES="Cargo.lock,package-lock.json,yarn.lock,pnpm-lock.yaml,.gitignore"
# --- End Configuration ---

total_items_listed_in_output=0
text_files_content_included=0
binary_files_marked=0
total_text_word_count=0
skipped_due_to_exclusion=0

> "$OUTPUT_FILENAME"
echo "Starting processing. Output will be in: $OUTPUT_FILENAME"

# Process files
# 1. find all files
# 2. grep -vE removes anything matching our exclusion regex
# 3. grep -v ignores the output file itself to prevent infinite loops
while IFS= read -r -d $'\0' filepath; do
    relative_filepath="${filepath#./}"
    [[ -z "$relative_filepath" ]] && continue

    # Double check exclusion for specific filenames
    filename_only=$(basename "$relative_filepath")
    if [[ ",$EXCLUDE_FILES," == *",$filename_only,"* ]]; then
        skipped_due_to_exclusion=$((skipped_due_to_exclusion + 1))
        continue
    fi

    [[ ! -f "$filepath" ]] && continue

    # Determine Type
    mime_type=$(file -b --mime-type "$filepath")
    extension_lower=$(echo "${relative_filepath##*.}" | tr '[:upper:]' '[:lower:]')

    case "$extension_lower" in
        txt|md|log|csv|tsv|json|xml|yaml|yml|ini|cfg|conf|sh|bash|zsh|csh|ksh|py|rb|pl|php|js|ts|jsx|tsx|html|htm|css|scss|less|java|c|cpp|cxx|h|hh|hpp|cs|go|rs|swift|kt|dart|sql|r|lua|gd|vue|svelte|env|example|dockerfile|mod|sum|toml|lock|gitignore|gitattributes|editorconfig|prettierrc|eslintrc|babelrc|postcssrc|Makefile|pem|crt|key|asc|svg) is_text_candidate=1 ;;
        *) [[ "$mime_type" == text/* || "$mime_type" == application/json ]] && is_text_candidate=1 || is_text_candidate=0 ;;
    esac

    is_binary=1
    if [[ "$is_text_candidate" -eq 1 ]]; then
        if cmp -s "$filepath" <(tr -d '\0' < "$filepath"); then is_binary=0; fi
    fi

    path_words=$(echo -n "$relative_filepath" | wc -w)
    header_overhead_words=$((2 + path_words))
    estimated_words=0
    [[ "$is_binary" -eq 0 ]] && estimated_words=$(wc -w < "$filepath")

    # Check word limit
    if (( total_items_listed_in_output > 0 && (total_text_word_count + estimated_words + header_overhead_words) > MAX_WORDS )); then
         echo -e "\n--- MAX WORD COUNT THRESHOLD REACHED ---" >> "$OUTPUT_FILENAME"; break
    fi

    # Log size
    fsize=$(stat -f%z "$filepath" 2>/dev/null || stat -c%s "$filepath" 2>/dev/null)
    echo "$fsize $relative_filepath" >> "$TEMP_SIZE_FILE"

    # Write Content
    total_items_listed_in_output=$((total_items_listed_in_output + 1))
    echo "File path: $relative_filepath" >> "$OUTPUT_FILENAME"
    echo '```' >> "$OUTPUT_FILENAME"

    if [[ "$is_binary" -eq 1 ]]; then
        echo "<BINARY CONTENT>" >> "$OUTPUT_FILENAME"
        binary_files_marked=$((binary_files_marked + 1))
    elif [[ ! -r "$filepath" ]]; then
        echo "<BINARY CONTENT - UNREADABLE>" >> "$OUTPUT_FILENAME"
        binary_files_marked=$((binary_files_marked + 1))
    else
        cat "$filepath" >> "$OUTPUT_FILENAME"
        text_files_content_included=$((text_files_content_included + 1))
        total_text_word_count=$((total_text_word_count + estimated_words))
    fi
    echo -e "\n\`\`\`\n" >> "$OUTPUT_FILENAME"

    [[ "$total_text_word_count" -ge "$MAX_WORDS" ]] && break

done < <(find . -type f -print0 | grep -zvE "$EXCLUDE_REGEX" | grep -zv "$OUTPUT_FILENAME")

# --- Summary ---
{
    echo -e "\n--- Summary ---"
    echo "Files skipped due to exclusion rules (est): $skipped_due_to_exclusion"
    echo "Total unique file paths listed in output: $total_items_listed_in_output"
    echo "  Text files with content included: $text_files_content_included"
    echo "  Files marked as binary: $binary_files_marked"
    echo "Total approximate text words written: $total_text_word_count / $MAX_WORDS"
    echo -e "\n--- Top 5 Largest Files Included ---"

    if [ -s "$TEMP_SIZE_FILE" ]; then
        sort -rn "$TEMP_SIZE_FILE" | head -n 5 | awk '{
            s=$1; u="B";
            if(s>=1024){s/=1024; u="KB"}
            if(s>=1024){s/=1024; u="MB"}
            if(s>=1024){s/=1024; u="GB"}
            name=$0; sub(/^[0-9]+ /, "", name);
            printf "%.2f %s\t%s\n", s, u, name
        }'
    else
        echo "None."
    fi
} >> "$OUTPUT_FILENAME"

rm -f "$TEMP_SIZE_FILE"
echo "Process complete. Summary appended to $OUTPUT_FILENAME"
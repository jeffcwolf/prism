//! Syntax-aware Rust source file parsing using `syn`.
//!
//! `SourceFile` wraps a parsed `syn::File` and provides access to the items
//! within: functions, structs, enums, traits, and impl blocks. This replaces
//! naive string-matching with accurate AST-based analysis.

use syn::visit::Visit;

use crate::types::AuditError;

/// A parsed Rust source file providing access to its syntactic items.
pub(crate) struct SourceFile {
    syntax: syn::File,
    source_text: String,
}

impl SourceFile {
    /// Parse a Rust source file from its text content.
    pub(crate) fn parse(source: &str) -> Result<Self, AuditError> {
        let syntax = syn::parse_file(source).map_err(|e| AuditError::ParseError {
            message: e.to_string(),
        })?;
        Ok(Self {
            syntax,
            source_text: source.to_string(),
        })
    }

    /// Parse a file from disk.
    pub(crate) fn from_path(path: &std::path::Path) -> Result<Self, AuditError> {
        let content = std::fs::read_to_string(path).map_err(|source| AuditError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::parse(&content)
    }

    /// Line count of the source text.
    pub(crate) fn line_count(&self) -> usize {
        self.source_text.lines().count()
    }

    /// Collect all function-like items (free functions, methods in impl blocks).
    pub(crate) fn functions(&self) -> Vec<FunctionItem> {
        let mut visitor = FunctionCollector::default();
        visitor.visit_file(&self.syntax);
        visitor.functions
    }

    /// Count items by visibility.
    pub(crate) fn item_counts(&self) -> ItemCounts {
        let mut counts = ItemCounts::default();
        for item in &self.syntax.items {
            counts.total += 1;
            if item_is_public(item) {
                counts.public += 1;
            }
            if matches!(item, syn::Item::Fn(_)) {
                counts.functions += 1;
            }
            // Count items inside impl blocks
            if let syn::Item::Impl(impl_block) = item {
                for impl_item in &impl_block.items {
                    counts.total += 1;
                    if impl_item_is_public(impl_item) {
                        counts.public += 1;
                    }
                    if matches!(impl_item, syn::ImplItem::Fn(_)) {
                        counts.functions += 1;
                    }
                }
            }
            // Count items inside trait definitions
            if let syn::Item::Trait(trait_def) = item {
                for trait_item in &trait_def.items {
                    counts.total += 1;
                    // Trait items are implicitly public if the trait is public
                    if matches!(item_visibility(item), Visibility::Public) {
                        counts.public += 1;
                    }
                    if matches!(trait_item, syn::TraitItem::Fn(_)) {
                        counts.functions += 1;
                    }
                }
            }
        }
        counts
    }
}

/// Counts of items by category within a source file.
#[derive(Debug, Default)]
pub(crate) struct ItemCounts {
    pub(crate) public: usize,
    pub(crate) total: usize,
    pub(crate) functions: usize,
}

/// A function extracted from the AST with its name and body.
pub(crate) struct FunctionItem {
    pub(crate) name: String,
    pub(crate) is_public: bool,
    pub(crate) body: syn::Block,
}

enum Visibility {
    Public,
    Restricted,
    Inherited,
}

fn item_visibility(item: &syn::Item) -> Visibility {
    match item {
        syn::Item::Fn(f) => vis_from_syn(&f.vis),
        syn::Item::Struct(s) => vis_from_syn(&s.vis),
        syn::Item::Enum(e) => vis_from_syn(&e.vis),
        syn::Item::Trait(t) => vis_from_syn(&t.vis),
        syn::Item::Type(t) => vis_from_syn(&t.vis),
        syn::Item::Const(c) => vis_from_syn(&c.vis),
        syn::Item::Static(s) => vis_from_syn(&s.vis),
        syn::Item::Mod(m) => vis_from_syn(&m.vis),
        syn::Item::Impl(_) => Visibility::Inherited,
        _ => Visibility::Inherited,
    }
}

fn vis_from_syn(vis: &syn::Visibility) -> Visibility {
    match vis {
        syn::Visibility::Public(_) => Visibility::Public,
        syn::Visibility::Restricted(_) => Visibility::Restricted,
        syn::Visibility::Inherited => Visibility::Inherited,
    }
}

fn item_is_public(item: &syn::Item) -> bool {
    matches!(
        item_visibility(item),
        Visibility::Public | Visibility::Restricted
    )
}

fn impl_item_is_public(item: &syn::ImplItem) -> bool {
    match item {
        syn::ImplItem::Fn(f) => !matches!(f.vis, syn::Visibility::Inherited),
        syn::ImplItem::Type(t) => !matches!(t.vis, syn::Visibility::Inherited),
        syn::ImplItem::Const(c) => !matches!(c.vis, syn::Visibility::Inherited),
        _ => false,
    }
}

#[derive(Default)]
struct FunctionCollector {
    functions: Vec<FunctionItem>,
}

impl<'ast> Visit<'ast> for FunctionCollector {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.functions.push(FunctionItem {
            name: node.sig.ident.to_string(),
            is_public: !matches!(node.vis, syn::Visibility::Inherited),
            body: (*node.block).clone(),
        });
        // Do not recurse into nested functions — they are separate items
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.functions.push(FunctionItem {
            name: node.sig.ident.to_string(),
            is_public: !matches!(node.vis, syn::Visibility::Inherited),
            body: node.block.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_file() {
        let sf = SourceFile::parse("").unwrap();
        assert_eq!(sf.functions().len(), 0);
        let counts = sf.item_counts();
        assert_eq!(counts.total, 0);
        assert_eq!(counts.public, 0);
    }

    #[test]
    fn parse_functions_and_items() {
        let source = r#"
            pub fn public_fn() {}
            fn private_fn() {}
            pub struct MyStruct;
            struct PrivateStruct;
            pub(crate) fn crate_fn() {}
        "#;
        let sf = SourceFile::parse(source).unwrap();
        let fns = sf.functions();
        assert_eq!(fns.len(), 3, "public_fn, private_fn, crate_fn");
        assert!(fns[0].is_public);
        assert!(!fns[1].is_public);
        assert!(fns[2].is_public); // pub(crate) counts as non-inherited

        let counts = sf.item_counts();
        assert_eq!(counts.public, 3, "pub fn, pub struct, pub(crate) fn");
        assert_eq!(counts.total, 5);
        assert_eq!(counts.functions, 3);
    }

    #[test]
    fn parse_impl_block_methods() {
        let source = r#"
            struct Foo;
            impl Foo {
                pub fn public_method(&self) {}
                fn private_method(&self) {}
            }
        "#;
        let sf = SourceFile::parse(source).unwrap();
        let fns = sf.functions();
        assert_eq!(fns.len(), 2);
        assert_eq!(fns[0].name, "public_method");
        assert!(fns[0].is_public);
        assert_eq!(fns[1].name, "private_method");
        assert!(!fns[1].is_public);
    }

    #[test]
    fn line_count_matches_source() {
        let source = "fn foo() {}\nfn bar() {}\nfn baz() {}\n";
        let sf = SourceFile::parse(source).unwrap();
        assert_eq!(sf.line_count(), 3);
    }

    #[test]
    fn parse_error_on_invalid_syntax() {
        let result = SourceFile::parse("fn {{{ invalid");
        assert!(result.is_err());
    }
}

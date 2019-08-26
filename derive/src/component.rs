//! Custom derive support for `abscissa_core::component::Component`.

use darling::{FromDeriveInput, FromMeta};
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use synstructure::Structure;

/// Custom derive for `abscissa_core::component::Component`
pub fn derive_component(s: Structure<'_>) -> TokenStream {
    let attrs = ComponentAttributes::from_derive_input(s.ast()).unwrap_or_else(|e| {
        panic!("error parsing component attributes: {}", e);
    });

    let name = &s.ast().ident;
    let abscissa_core = attrs.abscissa_core_crate();
    let dependency_methods = attrs.dependency_methods();

    s.gen_impl(quote! {
        gen impl<A> Component<A> for @Self
        where
            A: #abscissa_core::Application
        {
            #[doc = "Identifier for this component"]
            fn id(&self) -> #abscissa_core::component::Id {
                // TODO(tarcieri): use `core::any::type_name` here when stable
                #abscissa_core::component::Id::new(concat!(module_path!(), "::", stringify!(#name)))
            }

            #[doc = "Version of this component"]
            fn version(&self) -> #abscissa_core::Version {
                #abscissa_core::Version::parse(env!("CARGO_PKG_VERSION")).unwrap()
            }

            #dependency_methods
        }
    })
}

/// Parsed `#[component(...)]` attribute fields
#[derive(Debug, FromDeriveInput)]
#[darling(attributes(component))]
struct ComponentAttributes {
    /// Special attribute used by `abscissa_core` to `derive(Component)`.
    ///
    /// Workaround for using custom derive on traits defined in the same crate:
    /// <https://github.com/rust-lang/rust/issues/54363>
    #[darling(default)]
    core: bool,

    /// Dependent components to inject into the current component
    #[darling(multiple)]
    inject: Vec<InjectAttribute>,
}

impl ComponentAttributes {
    /// Ident for the `abscissa_core` crate.
    ///
    /// Allows `abscissa_core` itself to override this so it can consume its
    /// own traits/custom derives.
    pub fn abscissa_core_crate(&self) -> Ident {
        let crate_name = if self.core { "crate" } else { "abscissa_core" };

        Ident::new(crate_name, Span::call_site())
    }

    /// Generate `Component::dependencies()` and `register_dependencies()`
    pub fn dependency_methods(&self) -> TokenStream {
        if self.inject.is_empty() {
            return quote!();
        }

        let abscissa_core = self.abscissa_core_crate();
        let ids = self
            .inject
            .iter()
            .map(|inject| inject.id_tokens(&abscissa_core));

        let match_arms = self.inject.iter().map(|inject| inject.match_arm());

        quote! {
            fn dependencies(&self) -> std::slice::Iter<'_, #abscissa_core::component::Id> {
                const DEPENDENCIES: &[#abscissa_core::component::Id] = &[#(#ids),*];
                DEPENDENCIES.iter()
            }

            fn register_dependency(
                &mut self,
                handle: #abscissa_core::component::Handle,
                dependency: &mut dyn Component<A>,
            ) -> Result<(), FrameworkError> {
                match dependency.id().as_ref() {
                    #(#match_arms),*
                    _ => unreachable!()
                }
            }
        }
    }
}

/// Attribute declaring a dependency which should be injected
#[derive(Debug, FromMeta)]
pub struct InjectAttribute(String);

impl InjectAttribute {
    /// Parse an inject attribute into its component parse
    fn parse(&self) -> (&str, &str) {
        assert!(
            self.0.ends_with(')'),
            "expected {} to end with ')'",
            &self.0
        );

        let mut paren_parts = self.0[..(self.0.len() - 1)].split('(');
        let callback = paren_parts.next().unwrap();
        let component_id = paren_parts.next().unwrap();
        assert_eq!(paren_parts.next(), None);

        (callback, component_id)
    }

    /// Get the callback associated with this inject attribute
    pub fn callback(&self) -> Ident {
        Ident::new(self.parse().0, Span::call_site())
    }

    /// Get the component ID associated with this inject attribute
    pub fn component_id(&self) -> &str {
        self.parse().1
    }

    /// Get the tokens representing a component ID
    pub fn id_tokens(&self, abscissa_core: &Ident) -> TokenStream {
        let component_id = self.component_id();
        quote! { #abscissa_core::component::Id::new(#component_id) }
    }

    /// Get match arm that invokes a concrete callback
    pub fn match_arm(&self) -> TokenStream {
        let id_str = self.component_id();
        let callback = self.callback();

        quote! {
            #id_str => {
                let component_ref = (*dependency).as_mut_any().downcast_mut().unwrap();
                self.#callback(component_ref)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synstructure::test_derive;

    #[test]
    fn derive_component_struct() {
        test_derive! {
            derive_component {
                struct MyComponent {}
            }
            expands to {
                #[allow(non_upper_case_globals)]
                const _DERIVE_Component_A_FOR_MyComponent: () = {
                    impl<A> Component<A> for MyComponent
                    where
                        A: abscissa_core::Application
                    {
                        #[doc = "Identifier for this component" ]
                        fn id(&self) -> abscissa_core::component::Id {
                            abscissa_core::component::Id::new(
                                concat!(module_path!(), "::" , stringify!(MyComponent))
                            )
                        }

                        #[doc = "Version of this component"]
                        fn version(&self) -> abscissa_core::Version {
                            abscissa_core::Version::parse(env!("CARGO_PKG_VERSION")).unwrap()
                        }
                    }
                };
            }
            no_build // tests the code compiles are in the `abscissa` crate
        }
    }
}

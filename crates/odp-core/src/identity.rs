use crate::{
    ReferenceError, ResourceIdentity, ResourceType, derive_service_origin,
    is_local_resource_identifier,
};

impl ResourceIdentity {
    pub fn new(
        service_document_url: &str,
        resource_type: ResourceType,
        id: impl Into<String>,
    ) -> Result<Self, ReferenceError> {
        let id = id.into();
        if !is_local_resource_identifier(&id) {
            return Err(ReferenceError::InvalidResourceIdentifier(
                match resource_type {
                    ResourceType::Collection => crate::Operation::GetCollection,
                    ResourceType::Offering => crate::Operation::GetOffering,
                },
            ));
        }
        Ok(Self {
            id,
            service: derive_service_origin(service_document_url)?,
            resource_type,
        })
    }

    pub fn key(&self) -> String {
        let resource_type = match self.resource_type {
            ResourceType::Collection => "collection",
            ResourceType::Offering => "offering",
        };
        format!("{}\0{}\0{}", self.service, resource_type, self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composes_global_resource_identity() {
        let identity = ResourceIdentity::new(
            "https://shop.example/.well-known/odp",
            ResourceType::Offering,
            "plant-1",
        )
        .unwrap();
        assert_eq!(identity.service, "https://shop.example");
        assert_eq!(identity.key(), "https://shop.example\0offering\0plant-1");
    }
}

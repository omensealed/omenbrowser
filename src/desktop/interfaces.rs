use iced::Task;

use super::{DesktopApp, Message};

impl DesktopApp {
    pub(super) fn dispatch_interface_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::CreateTcpClientInterface => {
                self.update_create_tcp_client_interface();
                Ok(Task::none())
            }
            Message::CreateI2pInterface => {
                self.update_create_i2p_interface();
                Ok(Task::none())
            }
            Message::CreateRNodeInterface => {
                self.update_create_rnode_interface();
                Ok(Task::none())
            }
            Message::CreateGatewayPreset(gateway_id) => {
                self.update_create_gateway_preset(gateway_id);
                Ok(Task::none())
            }
            Message::SelectInterfaceProfile(index) => {
                self.update_select_interface_profile(index);
                Ok(Task::none())
            }
            Message::ToggleInterfaceEnabled(index) => {
                self.update_toggle_interface_enabled(index);
                Ok(Task::none())
            }
            Message::DeleteInterfaceProfile(index) => {
                self.update_delete_interface_profile(index);
                Ok(Task::none())
            }
            Message::ConfirmInterfaceDelete => {
                self.update_confirm_interface_delete();
                Ok(Task::none())
            }
            Message::CancelInterfaceDelete => {
                self.update_cancel_interface_delete();
                Ok(Task::none())
            }
            Message::InterfaceNameChanged { profile_id, value } => {
                self.update_interface_name_changed(profile_id, value);
                Ok(Task::none())
            }
            Message::TcpClientHostChanged { profile_id, value } => {
                self.update_tcp_client_host_changed(profile_id, value);
                Ok(Task::none())
            }
            Message::TcpClientPortChanged { profile_id, value } => {
                self.update_tcp_client_port_changed(profile_id, value);
                Ok(Task::none())
            }
            Message::TcpClientIfacNetworkChanged { profile_id, value } => {
                self.update_tcp_client_ifac_network_changed(profile_id, value);
                Ok(Task::none())
            }
            Message::TcpClientIfacPassphraseChanged { profile_id, value } => {
                self.update_tcp_client_ifac_passphrase_changed(profile_id, value);
                Ok(Task::none())
            }
            Message::TcpServerHostChanged { profile_id, value } => {
                self.update_tcp_server_host_changed(profile_id, value);
                Ok(Task::none())
            }
            Message::TcpServerPortChanged { profile_id, value } => {
                self.update_tcp_server_port_changed(profile_id, value);
                Ok(Task::none())
            }
            Message::TcpServerIfacNetworkChanged { profile_id, value } => {
                self.update_tcp_server_ifac_network_changed(profile_id, value);
                Ok(Task::none())
            }
            Message::TcpServerIfacPassphraseChanged { profile_id, value } => {
                self.update_tcp_server_ifac_passphrase_changed(profile_id, value);
                Ok(Task::none())
            }
            Message::ToggleI2pConnectable(index) => {
                self.update_toggle_i2p_connectable(index);
                Ok(Task::none())
            }
            Message::I2pPeersChanged { profile_id, value } => {
                self.update_i2p_peers_changed(profile_id, value);
                Ok(Task::none())
            }
            Message::RNodeDevicePortChanged { profile_id, value } => {
                self.update_rnode_device_port_changed(profile_id, value);
                Ok(Task::none())
            }
            Message::RNodeFrequencyChanged { profile_id, value } => {
                self.update_rnode_frequency_changed(profile_id, value);
                Ok(Task::none())
            }
            Message::RNodeBandwidthChanged { profile_id, value } => {
                self.update_rnode_bandwidth_changed(profile_id, value);
                Ok(Task::none())
            }
            Message::RNodeTxPowerChanged { profile_id, value } => {
                self.update_rnode_tx_power_changed(profile_id, value);
                Ok(Task::none())
            }
            Message::RNodeSpreadingFactorChanged { profile_id, value } => {
                self.update_rnode_spreading_factor_changed(profile_id, value);
                Ok(Task::none())
            }
            Message::RNodeCodingRateChanged { profile_id, value } => {
                self.update_rnode_coding_rate_changed(profile_id, value);
                Ok(Task::none())
            }
            _ => Err(message),
        }
    }

    pub(super) fn update_create_tcp_client_interface(&mut self) {
        self.app.create_tcp_client_interface_profile();
    }

    pub(super) fn update_create_i2p_interface(&mut self) {
        self.app.create_i2p_interface_profile();
    }

    pub(super) fn update_create_rnode_interface(&mut self) {
        self.app.create_rnode_interface_profile();
    }

    pub(super) fn update_create_gateway_preset(&mut self, gateway_id: String) {
        self.app.create_gateway_interface_profile(&gateway_id);
    }

    pub(super) fn update_select_interface_profile(&mut self, index: usize) {
        self.app.select_interface_profile(index);
    }

    pub(super) fn update_toggle_interface_enabled(&mut self, index: usize) {
        self.app.select_interface_profile(index);
        self.app.toggle_selected_interface_enabled();
    }

    pub(super) fn update_delete_interface_profile(&mut self, index: usize) {
        self.app.select_interface_profile(index);
        self.app.begin_selected_interface_delete_flow();
    }

    pub(super) fn update_confirm_interface_delete(&mut self) {
        self.app.confirm_pending_interface_delete();
    }

    pub(super) fn update_cancel_interface_delete(&mut self) {
        self.app.cancel_pending_interface_delete();
    }

    pub(super) fn update_interface_name_changed(&mut self, profile_id: String, value: String) {
        self.app.update_interface_profile_name(&profile_id, value);
    }

    pub(super) fn update_tcp_client_host_changed(&mut self, profile_id: String, value: String) {
        self.app
            .update_tcp_client_interface_host(&profile_id, value);
    }

    pub(super) fn update_tcp_client_port_changed(&mut self, profile_id: String, value: String) {
        self.app
            .update_tcp_client_interface_port(&profile_id, value);
    }

    pub(super) fn update_tcp_client_ifac_network_changed(
        &mut self,
        profile_id: String,
        value: String,
    ) {
        self.app
            .update_tcp_client_interface_ifac_network_name(&profile_id, value);
    }

    pub(super) fn update_tcp_client_ifac_passphrase_changed(
        &mut self,
        profile_id: String,
        value: String,
    ) {
        self.app
            .update_tcp_client_interface_ifac_passphrase(&profile_id, value);
    }

    pub(super) fn update_tcp_server_host_changed(&mut self, profile_id: String, value: String) {
        self.app
            .update_tcp_server_interface_host(&profile_id, value);
    }

    pub(super) fn update_tcp_server_port_changed(&mut self, profile_id: String, value: String) {
        self.app
            .update_tcp_server_interface_port(&profile_id, value);
    }

    pub(super) fn update_tcp_server_ifac_network_changed(
        &mut self,
        profile_id: String,
        value: String,
    ) {
        self.app
            .update_tcp_server_interface_ifac_network_name(&profile_id, value);
    }

    pub(super) fn update_tcp_server_ifac_passphrase_changed(
        &mut self,
        profile_id: String,
        value: String,
    ) {
        self.app
            .update_tcp_server_interface_ifac_passphrase(&profile_id, value);
    }

    pub(super) fn update_toggle_i2p_connectable(&mut self, index: usize) {
        self.app.select_interface_profile(index);
        self.app.toggle_selected_interface_connectable();
    }

    pub(super) fn update_i2p_peers_changed(&mut self, profile_id: String, value: String) {
        self.app.update_i2p_interface_peers(&profile_id, value);
    }

    pub(super) fn update_rnode_device_port_changed(&mut self, profile_id: String, value: String) {
        self.app
            .update_rnode_interface_device_port(&profile_id, value);
    }

    pub(super) fn update_rnode_frequency_changed(&mut self, profile_id: String, value: String) {
        self.app
            .update_rnode_interface_frequency(&profile_id, value);
    }

    pub(super) fn update_rnode_bandwidth_changed(&mut self, profile_id: String, value: String) {
        self.app
            .update_rnode_interface_bandwidth(&profile_id, value);
    }

    pub(super) fn update_rnode_tx_power_changed(&mut self, profile_id: String, value: String) {
        self.app.update_rnode_interface_tx_power(&profile_id, value);
    }

    pub(super) fn update_rnode_spreading_factor_changed(
        &mut self,
        profile_id: String,
        value: String,
    ) {
        self.app
            .update_rnode_interface_spreading_factor(&profile_id, value);
    }

    pub(super) fn update_rnode_coding_rate_changed(&mut self, profile_id: String, value: String) {
        self.app
            .update_rnode_interface_coding_rate(&profile_id, value);
    }
}

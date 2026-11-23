# 安卓实机逐项结果

共 154 个场景，累计通过 149，剩余 5。首轮通过 121 个，本轮专门复测首轮的 33 个失败场景。场景列保留首轮仪器日志，末列链接本轮证据。

| 测试与场景 | 用例 | 当前状态 | 复测证据或未通过原因 |
| --- | --- | --- | --- |
| [AppUpdateTest](final-AppUpdateTest.txt) · 全功能包 | cachedInstalledVersionCannotLeaveAnUpdateBadgeAfterUpgrade | [x] 通过 |  |
| [AppUpdateTest](final-AppUpdateTest.txt) · 全功能包 | onlyPublishedStableReleasesWithExactAssetAreAccepted | [x] 通过 |  |
| [AppUpdateTest](final-AppUpdateTest.txt) · 全功能包 | slowFailureDoesNotBlockMainThreadAndManualRetryWorks | [x] 通过 |  |
| [AppUpdateTest](final-AppUpdateTest.txt) · 全功能包 | throttleBackoffAndClockRecovery | [x] 通过 |  |
| [AppUpdateTest](final-AppUpdateTest.txt) · 全功能包 | versionsCompatibilityAndMalformedMetadata | [x] 通过 |  |
| [BrowserHostTest](final-BrowserHostTest.txt) · ColorOS 浏览器 | addressBarCommitsChineseAndEnglishWithoutLosingFocus | [x] 通过 | [复测 #1](retest/browser-final.jsonl) |
| [BrowserHostTest](final-BrowserHostTest.txt) · ColorOS 浏览器 | enterCommitsBeforeSendAndEmptyTemporaryDoesNotSubmit | [x] 通过 | [复测 #2](retest/browser-all-windows.jsonl) |
| [BrowserHostTest](independent-webview-BrowserHostTest%23enterCommitsBeforeSendAndEmptyTemporaryDoesNotSubmit.txt) · 独立系统 WebView | enterCommitsBeforeSendAndEmptyTemporaryDoesNotSubmit | [x] 通过 |  |
| [BrowserHostTest](final-BrowserHostTest.txt) · ColorOS 浏览器 | hideRotateAndSwitchEditorsDoNotReuseOldCandidates | [x] 通过 | [复测 #3](retest/browser-final.jsonl) |
| [BrowserHostTest](independent-webview-BrowserHostTest%23hideRotateAndSwitchEditorsDoNotReuseOldCandidates.txt) · 独立系统 WebView | hideRotateAndSwitchEditorsDoNotReuseOldCandidates | [x] 通过 | [复测 #4](retest/4-hideRotateAndSwitchEditorsDoNotReuseOldCandidates.txt) |
| [BrowserHostTest](final-BrowserHostTest.txt) · ColorOS 浏览器 | restrictedFieldsAndNextActionInChrome | [ ] 未通过 | ColorOS 密码框由 com.oplus.securitykeyboard 接管，MyWubi 的 g 键不可用。数字、电话、网址、邮箱和下一项这五组通过。 [复测 #5](retest/browser-final.jsonl) |
| [BrowserHostTest](independent-webview-BrowserHostTest%23restrictedFieldsAndNextActionInChrome.txt) · 独立系统 WebView | restrictedFieldsAndNextActionInChrome | [ ] 未通过 | 系统 WebView 的密码框由 ColorOS 安全键盘接管。数字、电话、网址、邮箱和下一项这五组通过。 [复测 #6](retest/6-restrictedFieldsAndNextActionInChrome-130636.txt) |
| [BrowserHostTest](final-BrowserHostTest.txt) · ColorOS 浏览器 | unicodeSelectionAndCompositionCursorInChrome | [x] 通过 | [复测 #7](retest/browser-all-windows.jsonl) |
| [BrowserHostTest](independent-webview-BrowserHostTest%23unicodeSelectionAndCompositionCursorInChrome.txt) · 独立系统 WebView | unicodeSelectionAndCompositionCursorInChrome | [x] 通过 |  |
| [ClipboardSuggestionTest](final-ClipboardSuggestionTest.txt) · 全功能包 | previewIsBoundedButPasteKeepsFullTextAndSessionDismisses | [x] 通过 |  |
| [ClipboardSuggestionTest](final-ClipboardSuggestionTest.txt) · 全功能包 | sensitiveDisabledEmptyAndNonTextClipsNeverPreview | [x] 通过 |  |
| [CustomKeyboardHostTest](final-CustomKeyboardHostTest.txt) · 全功能包 | realTemplatesKeepGroupsLocalAndReturnToRememberedMode | [ ] 未通过 | 26/14/9 键模板、布局恢复、分组选词、普通和受限字段已执行。唯一未通过的是原生密码框 inputType=129，没有出现 MyWubi 的 q 键。 [复测 #8](retest/8-realTemplatesKeepGroupsLocalAndReturnToRememberedMode-133941.txt) |
| [DeviceBenchmarkTest](final-DeviceBenchmarkTest.txt) · 全功能包 | recordRealDictionaryJniLatencyAndMemory | [x] 通过 |  |
| [EditorContractTest](final-EditorContractTest.txt) · 全功能包 | batchBoundaryFailuresRemainUnknownWithoutRetry | [x] 通过 |  |
| [EditorContractTest](final-EditorContractTest.txt) · 全功能包 | clearSelectsWholeEditorAndStopsWhenSelectionFailsOrContextChanges | [x] 通过 |  |
| [EditorContractTest](final-EditorContractTest.txt) · 全功能包 | clearingCompositionReleasesOwnershipEvenWhenFinishFails | [x] 通过 |  |
| [EditorContractTest](final-EditorContractTest.txt) · 全功能包 | inputPurposeAndActionAreIndependentOfBaseMode | [x] 通过 |  |
| [EditorContractTest](final-EditorContractTest.txt) · 全功能包 | pasteCommitsLiteralTextExactlyOnceWithoutClipboardSideEffects | [x] 通过 |  |
| [EditorContractTest](final-EditorContractTest.txt) · 全功能包 | unicodeDeletionUsesWholeGraphemes | [x] 通过 |  |
| [ExpandedCandidatesTest](final-ExpandedCandidatesTest.txt) · 全功能包 | expandedCandidatesAppearOnceAndReturnToTyping | [x] 通过 |  |
| [ExpandedCandidatesTest](final-ExpandedCandidatesTest.txt) · 全功能包 | expandedCandidatesRemainUsableAtLargeFont | [x] 通过 |  |
| [ExpandedCandidatesTest](final-ExpandedCandidatesTest.txt) · 全功能包 | expandedGridScrollsAndPagesWithCurrentCandidateIdentity | [x] 通过 |  |
| [ExpressionBenchmarkTest](final-ExpressionBenchmarkTest.txt) · 全功能包 | compareAppendSwitchesWithFullLocalLibraries | [x] 通过 |  |
| [ExpressionFontTest](final-ExpressionFontTest.txt) · 全功能包 | fullLibrariesKeepOriginalSequencesAndReportMissingSystemGlyphs | [x] 通过 |  |
| [ExpressionHostTest](final-ExpressionHostTest.txt) · 全功能包 | browseWithoutTypingCommitsRepeatedSymbolsAndRetainsWindow | [x] 通过 |  |
| [ExpressionHostTest](final-ExpressionHostTest.txt) · 全功能包 | fullLibraryTailSelectionAndModeReturnDoNotLeakPartialQueries | [x] 通过 |  |
| [ExpressionHostTest](system-font-ExpressionHostTest%23fullLibraryTailSelectionAndModeReturnDoNotLeakPartialQueries.txt) · 深色 · 1.0 倍 | fullLibraryTailSelectionAndModeReturnDoNotLeakPartialQueries | [x] 通过 |  |
| [ExpressionHostTest](oem-large-font-ExpressionHostTest%23fullLibraryTailSelectionAndModeReturnDoNotLeakPartialQueries.txt) · 深色 · 系统 1.6 倍 | fullLibraryTailSelectionAndModeReturnDoNotLeakPartialQueries | [x] 通过 |  |
| [ExpressionHostTest](final-ExpressionHostTest.txt) · 全功能包 | kanaLiteralAliasUsesVisibleCategoryEntryAndCommitsOnlyTheResult | [x] 通过 | [复测 #9](retest/9-kanaLiteralAliasUsesVisibleCategoryEntryAndCommitsOnlyTheResult-125330.txt) |
| [ExpressionHostTest](final-ExpressionHostTest.txt) · 全功能包 | numericChoicesStayLocalAndResourceReloadRejectsAnOldAccessibilityClick | [x] 通过 |  |
| [ExpressionHostTest](final-ExpressionHostTest.txt) · 全功能包 | realQueryEditsNeverTouchHostAndFinalEmojiAndLongKaomojiStayComplete | [x] 通过 | [复测 #10](retest/10-realQueryEditsNeverTouchHostAndFinalEmojiAndLongKaomojiStayComplete-135401.txt) |
| [ExpressionLayoutTest](final-ExpressionLayoutTest.txt) · 全功能包 | browserLargeLandscape | [x] 通过 | [复测 #11](retest/11-browserLargeLandscape.txt) |
| [ExpressionLayoutTest](final-ExpressionLayoutTest.txt) · 全功能包 | browserPortrait | [x] 通过 |  |
| [ExpressionLayoutTest](final-ExpressionLayoutTest.txt) · 全功能包 | landscapeLargeFont | [x] 通过 | [复测 #12](retest/12-landscapeLargeFont.txt) |
| [ExpressionLayoutTest](final-ExpressionLayoutTest.txt) · 全功能包 | landscapeNormalFont | [x] 通过 | [复测 #13](retest/13-landscapeNormalFont.txt) |
| [ExpressionLayoutTest](final-ExpressionLayoutTest.txt) · 全功能包 | portraitLargeFont | [x] 通过 |  |
| [ExpressionLayoutTest](final-ExpressionLayoutTest.txt) · 全功能包 | portraitNormalFont | [x] 通过 |  |
| [ExpressionQueryTest](final-ExpressionQueryTest.txt) · 全功能包 | multiwordQueryStaysLocalAndOnlyTheCompleteExpressionCrossesJni | [x] 通过 |  |
| [ExpressionResourceTest](final-ExpressionResourceTest.txt) · 全功能包 | bundledCompleteLibrariesAndNoticesAreAvailableOffline | [x] 通过 |  |
| [ExpressionResourceTest](final-ExpressionResourceTest.txt) · 全功能包 | importSurvivesSourceRemovalAndSelectionSurvivesRestartAndMainTableChange | [x] 通过 |  |
| [ExpressionResourceTest](final-ExpressionResourceTest.txt) · 全功能包 | independentSwitchesActivateWithBuiltinPathsFromTemporaryValidationDirectory | [x] 通过 |  |
| [ExpressionResourceTest](final-ExpressionResourceTest.txt) · 全功能包 | wrongTypeAndCorruptResourceNeverOverwriteActiveConfigAndPreparationPreservesFiles | [x] 通过 |  |
| [ExternalHostTest](final-ExternalHostTest.txt) · 全功能包 | composeExpansionAndRepeatStopsOnRelease | [x] 通过 | [复测 #14](retest/14-composeExpansionAndRepeatStopsOnRelease.txt) |
| [ExternalHostTest](final-ExternalHostTest.txt) · 全功能包 | shapeInputInSystemSettings | [x] 通过 | [复测 #15](retest/15-shapeInputInSystemSettings-135519.txt) |
| [GroupProtocolTest](final-GroupProtocolTest.txt) · 全功能包 | groupsMatchSharedRustFixturesAndNeverBecomeHostText | [x] 通过 |  |
| [GroupProtocolTest](final-GroupProtocolTest.txt) · 全功能包 | sharedGestureTracesMatchDesktopSimulation | [x] 通过 |  |
| [HideKeyboardHostTest](final-HideKeyboardHostTest.txt) · 全功能包 | landscapeKeepsBottomEditorVisibleWhileTyping | [x] 通过 |  |
| [HideKeyboardHostTest](final-HideKeyboardHostTest.txt) · 全功能包 | toolbarHidesRealImeAndEditorCanOpenItAgain | [x] 通过 |  |
| [KeyboardBenchmarkTest](final-KeyboardBenchmarkTest.txt) · 全功能包 | compareExactAndGroupedJniOnLocalRealTables | [x] 通过 |  |
| [KeyboardLayoutTest](final-KeyboardLayoutTest.txt) · 全功能包 | compositionKeepsKeyboardHeightAndKeyPositionsStable | [x] 通过 |  |
| [KeyboardLayoutTest](final-KeyboardLayoutTest.txt) · 全功能包 | directFieldsUseAsciiSymbolsAndExpandedCandidatesKeepBackspace | [x] 通过 |  |
| [KeyboardLayoutTest](final-KeyboardLayoutTest.txt) · 全功能包 | hideKeyboardIsAvailableAcrossInputStatesAndDoesNotDispatchText | [x] 通过 |  |
| [KeyboardLayoutTest](final-KeyboardLayoutTest.txt) · 全功能包 | landscapeCandidatesStayAboveProportionalKeys | [x] 通过 | [复测 #16](retest/16-landscapeCandidatesStayAboveProportionalKeys.txt) |
| [KeyboardLayoutTest](final-KeyboardLayoutTest.txt) · 全功能包 | largeFontCompositionKeepsKeyboardHeightAndKeyPositionsStable | [x] 通过 |  |
| [KeyboardLayoutTest](final-KeyboardLayoutTest.txt) · 全功能包 | staggeredLettersAndSeparatePanelsPreserveActions | [x] 通过 |  |
| [KeyboardPackageTest](final-KeyboardPackageTest.txt) · 全功能包 | templateImportDecodeSaveExportAndReimportPreserveCombination | [x] 通过 |  |
| [KeyboardPackageTest](final-KeyboardPackageTest.txt) · 全功能包 | windowsEditorArtifactPreservesEveryEntryAcrossAndroidExport | [x] 通过 |  |
| [KeyboardShortcutsHostTest](final-KeyboardShortcutsHostTest.txt) · 全功能包 | realEditorPastesDismissesAndClearsWholeFieldFromMiddle | [x] 通过 |  |
| [KeyboardTouchTest](final-KeyboardTouchTest.txt) · 全功能包 | deleteHoldSwipeReleaseReturnAndContextCancellation | [x] 通过 |  |
| [KeyboardTouchTest](final-KeyboardTouchTest.txt) · 全功能包 | eightDirectionsMergedUpAndReturnToCenterUseOneReleaseAction | [x] 通过 |  |
| [KeyboardTouchTest](final-KeyboardTouchTest.txt) · 全功能包 | modesUseFixedLabelAndSymbolsDoNotConsumeOneShotShift | [x] 通过 | [复测 #17](retest/17-modesUseFixedLabelAndSymbolsDoNotConsumeOneShotShift.txt) |
| [KeyboardTouchTest](final-KeyboardTouchTest.txt) · 全功能包 | overlappingAlternatingFingersDoNotDropRepeatedKeys | [x] 通过 | [复测 #18](retest/18-overlappingAlternatingFingersDoNotDropRepeatedKeys-130055.txt) |
| [KeyboardTouchTest](final-KeyboardTouchTest.txt) · 全功能包 | previewDoesNotBlockRapidTypingAndHidingCancelsHeldKey | [x] 通过 |  |
| [KeyboardTouchTest](final-KeyboardTouchTest.txt) · 全功能包 | tapPreviewSwipeHoldAndCancellationCommitExactlyOnce | [x] 通过 | [复测 #19](retest/19-tapPreviewSwipeHoldAndCancellationCommitExactlyOnce-130101.txt) |
| [KeyboardTouchTest](final-KeyboardTouchTest.txt) · 全功能包 | visualKeyGuttersStillAcceptTaps | [x] 通过 |  |
| [LibraryContractTest](final-LibraryContractTest.txt) · 全功能包 | userCrudConflictImportAndExportUseSharedRules | [x] 通过 |  |
| [LibraryModelTest](final-LibraryModelTest.txt) · 全功能包 | auxiliaryChoicesSaveRelativeReferencesAndPreserveHiddenDesktopKey | [x] 通过 |  |
| [LibraryModelTest](final-LibraryModelTest.txt) · 全功能包 | invalidSaveRetainsDraftAndConflictCannotOverwriteDisk | [x] 通过 |  |
| [LibraryUiTest](final-LibraryUiTest.txt) · 全功能包 | metadataUsesLabeledRadioChoicesAndAuxiliaryPickers | [x] 通过 |  |
| [LifecycleContractTest](final-LifecycleContractTest.txt) · 全功能包 | contextSwitchDuringCommitCancelsRemainingSteps | [x] 通过 |  |
| [LifecycleContractTest](final-LifecycleContractTest.txt) · 全功能包 | lateBuildAndDestroyedOwnerCannotPublish | [x] 通过 |  |
| [LifecycleContractTest](final-LifecycleContractTest.txt) · 全功能包 | localPreeditBusyAndUnknownCommitRecoverWithoutReplay | [x] 通过 |  |
| [LifecycleContractTest](final-LifecycleContractTest.txt) · 全功能包 | overflowIsExplicitAndDoesNotReplaceQueuedKeys | [x] 通过 |  |
| [LifecycleContractTest](final-LifecycleContractTest.txt) · 全功能包 | reentrantKeysRemainOrderedAndControlEventsInterrupt | [x] 通过 |  |
| [LifecycleContractTest](final-LifecycleContractTest.txt) · 全功能包 | unknownWriteCannotRetainAnOldComposition | [x] 通过 |  |
| [MainKeyboardMenuTest](final-MainKeyboardMenuTest.txt) · 全功能包 | menuKeepsInputModeAndHandlesCancelUnavailableAndLastLayout | [x] 通过 |  |
| [ModeHostTest](final-ModeHostTest.txt) · 全功能包 | englishTemporaryPinyinAndDigitsInExternalHost | [x] 通过 |  |
| [ModelBackendTest](final-ModelBackendTest.txt) · 全功能包 | catalogUsesTheEngineDirectoryAndProtectsSavedSelection | [x] 通过 |  |
| [ModelBackendTest](final-ModelBackendTest.txt) · 全功能包 | llamaCpuScoresAnExplicitlyProvisionedLicensedPackage | [x] 通过 |  |
| [ModelBackendTest](final-ModelBackendTest.txt) · 全功能包 | onnxCpuLoadsScoresAndReleasesThroughNativeWorker | [x] 通过 |  |
| [ModelBackendTest](final-ModelBackendTest.txt) · 全功能包 | preparedGenerationResponsesMeetDeadline | [x] 通过 |  |
| [ModelDownloadUiTest](final-ModelDownloadUiTest.txt) · 全功能包 | downloadChooseSaveReopenAndPredictInExternalEditor | [x] 通过 | [复测 #20](retest/20-downloadChooseSaveReopenAndPredictInExternalEditor-132825.txt) |
| [ModelTrialTest](final-ModelTrialTest.txt) · 全功能包 | explicitUiSampleRunsWithoutSelectingOrEnablingModel | [x] 通过 | [复测 #21](retest/21-explicitUiSampleRunsWithoutSelectingOrEnablingModel-132015.txt) |
| [ModelTrialTest](final-ModelTrialTest.txt) · 全功能包 | importAndTrialShareFailureDiagnosticsAndPreserveConfiguration | [x] 通过 |  |
| [NativeContractTest](final-NativeContractTest.txt) · 全功能包 | customCodesAndUnavailableModeKeepTheRound | [x] 通过 |  |
| [NativeContractTest](final-NativeContractTest.txt) · 全功能包 | protocolBoundariesAndLifecycle | [x] 通过 |  |
| [PersonalDataTest](final-PersonalDataTest.txt) · 全功能包 | captureTrainExchangeAndPrivacyUseTheNativeProductPath | [x] 通过 |  |
| [PersonalDataTest](final-PersonalDataTest.txt) · 全功能包 | shortAndMissingHostReadsKeepTheirMeaningAcrossEditors | [x] 通过 |  |
| [PersonalDataTest](final-PersonalDataTest.txt) · 全功能包 | windowsPackagesRoundTripThroughAndroidWithoutDuplicatingEvents | [x] 通过 |  |
| [PersonalDeviceSoakTest](final-PersonalDeviceSoakTest.txt) · 全功能包 | backgroundLearningDuringThirtyMinutesOfRealEditorInput | [x] 通过 |  |
| [PersonalDeviceSoakTest](final-PersonalDeviceSoakTest%23manualPauseAndResumeAfterCompletedSyntheticSoak.txt) · 全功能包 | manualPauseAndResumeAfterCompletedSyntheticSoak | [x] 通过 |  |
| [PersonalHostTest](final-PersonalHostTest.txt) · 全功能包 | personalCandidateCommitsAndCollectionCanBeStoppedIndependently | [x] 通过 |  |
| [PhysicalEditCaptureTest](final-PhysicalEditCaptureTest.txt) · 全功能包 | selectionExternalReplacementAndReconnectionUseTheRealHost | [x] 通过 | [复测 #22](retest/22-selectionExternalReplacementAndReconnectionUseTheRealHost-125521.txt) |
| [PhysicalKeyboardMatrixTest](final-PhysicalKeyboardMatrixTest.txt) · 全功能包 | gesturesCommitWithEachSkinLayoutThemeAndOrientation | [x] 通过 |  |
| [PhysicalLayoutComparisonTest](final-PhysicalLayoutComparisonTest.txt) · 全功能包 | identicalWordsOnThreeLayouts | [x] 通过 | [复测 #23](retest/23-identicalWordsOnThreeLayouts-135630.txt) |
| [PhysicalModelDurationTest](final-PhysicalModelDurationTest.txt) · 全功能包 | thirtyMinutesOfCpuPredictionsWithResourceSamples | [x] 通过 |  |
| [PhysicalPresentationTest](final-PhysicalPresentationTest.txt) · 全功能包 | tapToCapturedCandidateAndVibrationSwitch | [ ] 未通过 | 振动开关两种状态均通过。三轮从触摸到截取匹配候选画面的 P95 为 380.7、325.0、307.0 ms，P99 为 448.2、342.8、363.7 ms，超出 200/300 ms 门槛。 [复测 #24](retest/24-tapToCapturedCandidateAndVibrationSwitch-135022.txt) |
| [PhysicalRuleEditorTest](system-font-PhysicalRuleEditorTest.txt) · 深色 · 1.0 倍 | errorAndSaveAreReachableAboveTheNumericKeyboard | [x] 通过 | [复测 #25](retest/25-errorAndSaveAreReachableAboveTheNumericKeyboard.txt) |
| [PhysicalRuleEditorTest](oem-large-font-PhysicalRuleEditorTest.txt) · 深色 · 系统 1.6 倍 | errorAndSaveAreReachableAboveTheNumericKeyboard | [x] 通过 | [复测 #26](retest/26-errorAndSaveAreReachableAboveTheNumericKeyboard-134149.txt) |
| [PredictionHostTest](final-PredictionHostTest.txt) · 全功能包 | confirmedWordsPredictAndNewCodesOrExternalEditsDiscardSuggestions | [x] 通过 |  |
| [PredictionHostTest](final-PredictionHostTest.txt) · 全功能包 | generatedDictionaryExternalWordsAppendInTheRealHost | [x] 通过 | [复测 #27](retest/27-generatedDictionaryExternalWordsAppendInTheRealHost-135837.txt) |
| [PredictionHostTest](final-PredictionHostTest.txt) · 全功能包 | lowConfidenceModelPredictionsStayHiddenAndSpaceRemainsLiteral | [x] 通过 |  |
| [PredictionHostTest](final-PredictionHostTest.txt) · 全功能包 | modelScoredPredictionsUseTheConfirmedHostCommitPath | [x] 通过 |  |
| [PredictionHostTest](final-PredictionHostTest.txt) · 全功能包 | preloadedModelRecoversAfterImeHideAndMemoryTrim | [x] 通过 |  |
| [ResourceContractTest](final-ResourceContractTest.txt) · 全功能包 | activationInterruptionKeepsOldOrCompleteNewGeneration | [x] 通过 |  |
| [ResourceContractTest](final-ResourceContractTest.txt) · 全功能包 | badReferencesAndConflictsPreserveActiveConfig | [x] 通过 |  |
| [ResourceContractTest](final-ResourceContractTest.txt) · 全功能包 | defaultsNeverOverwriteUserFilesAndTouchSaveIgnoresDesktopKeys | [x] 通过 |  |
| [ResourceContractTest](final-ResourceContractTest.txt) · 全功能包 | importOnlyAddsChoicesAndBothImportedAndDefaultCanBeSelectedAfterReopen | [x] 通过 |  |
| [ResourceContractTest](final-ResourceContractTest.txt) · 全功能包 | legacyImportsRemainSelectableAlongsideDefaults | [x] 通过 |  |
| [ResourceContractTest](final-ResourceContractTest.txt) · 全功能包 | specialInputUpgradeAndPortableRestorePreserveSelectionsAndRules | [x] 通过 |  |
| [SettingsModelTest](final-SettingsModelTest.txt) · 全功能包 | cancellingInvalidImportPreservesEditableDraftAcrossReopen | [x] 通过 |  |
| [SettingsModelTest](final-SettingsModelTest.txt) · 全功能包 | predictionSettingsPersistWithoutInstallingOrEnablingAModelImplicitly | [x] 通过 |  |
| [SettingsNavigationTest](final-SettingsNavigationTest.txt) · 全功能包 | categoriesKeepDraftAndSaveVisibleAfterScrollAndRotation | [x] 通过 |  |
| [SettingsNavigationTest](system-font-SettingsNavigationTest%23categoriesKeepDraftAndSaveVisibleAfterScrollAndRotation.txt) · 深色 · 1.0 倍 | categoriesKeepDraftAndSaveVisibleAfterScrollAndRotation | [x] 通过 |  |
| [SettingsNavigationTest](oem-large-font-SettingsNavigationTest%23categoriesKeepDraftAndSaveVisibleAfterScrollAndRotation.txt) · 深色 · 系统 1.6 倍 | categoriesKeepDraftAndSaveVisibleAfterScrollAndRotation | [x] 通过 |  |
| [SettingsNavigationTest](final-SettingsNavigationTest.txt) · 全功能包 | finalUnicodeCategoryIsSearchableAndKeepsItsOwnDraft | [x] 通过 |  |
| [SettingsNavigationTest](final-SettingsNavigationTest.txt) · 全功能包 | skinHasItsOwnPageAndKeepsAppearanceDraftAcrossNavigationAndRecreation | [x] 通过 |  |
| [SettingsNavigationTest](final-SettingsNavigationTest.txt) · 全功能包 | specialInputRecreationRetainsSelectionSearchAndQueryDraft | [x] 通过 |  |
| [SettingsNavigationTest](system-font-SettingsNavigationTest%23specialInputRecreationRetainsSelectionSearchAndQueryDraft.txt) · 深色 · 1.0 倍 | specialInputRecreationRetainsSelectionSearchAndQueryDraft | [x] 通过 |  |
| [SettingsNavigationTest](oem-large-font-SettingsNavigationTest%23specialInputRecreationRetainsSelectionSearchAndQueryDraft.txt) · 深色 · 系统 1.6 倍 | specialInputRecreationRetainsSelectionSearchAndQueryDraft | [x] 通过 |  |
| [SettingsNavigationTest](final-SettingsNavigationTest%23standardPackageExplainsModelLimitAndOffersWorkingDictionaryPath.txt) · 基础包 | standardPackageExplainsModelLimitAndOffersWorkingDictionaryPath | [x] 通过 |  |
| [SettingsProcessRestoreTest](final-SettingsProcessRestoreTest%23recoverPersistedDraftInNewProcess.txt) · 全功能包 | recoverPersistedDraftInNewProcess | [x] 通过 |  |
| [SettingsProcessRestoreTest](final-SettingsProcessRestoreTest%23saveSkinDraftInFirstProcess.txt) · 全功能包 | saveSkinDraftInFirstProcess | [x] 通过 |  |
| [SettingsUiTest](final-SettingsUiTest.txt) · 全功能包 | draftSurvivesRotationExitAndSafCancellation | [x] 通过 |  |
| [SettingsUiTest](final-SettingsUiTest.txt) · 全功能包 | safImportsExplicitMainAndAuxiliaryAndApplies | [x] 通过 |  |
| [SettingsUiTest](final-SettingsUiTest.txt) · 全功能包 | safSingleFileAutomaticallySelectsMainTable | [x] 通过 |  |
| [ShortcutSettingsTest](final-ShortcutSettingsTest.txt) · 全功能包 | rejectedSaveRestoresDraftAndCorrectedSaveCanonicalizesCode | [x] 通过 |  |
| [SkinContractTest](final-SkinContractTest.txt) · 全功能包 | actualImageSkinSupportsDarkLandscapeAndLargeFont | [x] 通过 | [复测 #28](retest/28-actualImageSkinSupportsDarkLandscapeAndLargeFont-125251.txt) |
| [SkinContractTest](final-SkinContractTest.txt) · 全功能包 | actualWindowsEditorExportResolvesAndRoundtripsWithoutChangingWindows | [x] 通过 |  |
| [SkinContractTest](final-SkinContractTest.txt) · 全功能包 | imageVariantsShareDecodedResourcesAndRejectCorruption | [x] 通过 |  |
| [SkinContractTest](final-SkinContractTest.txt) · 全功能包 | nativeKeyboardKeepsCandidatesAndTouchTargetsAcrossSkinAndLargeFontChanges | [x] 通过 | [复测 #29](retest/29-nativeKeyboardKeepsCandidatesAndTouchTargetsAcrossSkinAndLargeFontChanges-125300.txt) |
| [SkinContractTest](final-SkinContractTest.txt) · 全功能包 | realImeKeepsPendingCandidateAcrossAppearanceSave | [x] 通过 |  |
| [SkinContractTest](final-SkinContractTest.txt) · 全功能包 | resolvingBothVariantsPreservesCandidateIdentityAndPendingCommit | [x] 通过 |  |
| [SkinPerformanceTest](final-SkinPerformanceTest.txt) · 全功能包 | thousandImageReloadsReleaseRetiredBitmaps | [x] 通过 |  |
| [SkinSettingsUiTest](final-SkinSettingsUiTest.txt) · 全功能包 | safImportNativePreviewDraftSaveAndSystemShare | [x] 通过 |  |
| [SpecialDeviceBenchmarkTest](final-SpecialDeviceBenchmarkTest.txt) · 全功能包 | threeRunsOnArtWithTheFixedRuleAndCategoryCounts | [ ] 未通过 | 四组 JNI 延迟和全集构建达标。全集热重载时整进程峰值为 273.9、278.8、291.9 MiB，均超过 256 MiB；加载完成时为 233.9、237.1、271.0 MiB。 [复测 #30](retest/30-threeRunsOnArtWithTheFixedRuleAndCategoryCounts-134953.txt) |
| [SpecialPositionHostTest](final-SpecialPositionHostTest.txt) · 全功能包 | globalPositionsAndReachablePrefixCommitToTheExternalEditor | [x] 通过 | [复测 #31](retest/31-globalPositionsAndReachablePrefixCommitToTheExternalEditor-132620.txt) |
| [TableDisplayNameTest](final-TableDisplayNameTest.txt) · 全功能包 | catalogRereadsHeaderAndKeepsOriginalFilenameForBlankNames | [x] 通过 |  |
| [UpdateDownloadTest](final-UpdateDownloadTest.txt) · 全功能包 | archiveValidationRejectsHashPackageVersionAndSignerMismatch | [x] 通过 |  |
| [UpdateDownloadTest](final-UpdateDownloadTest.txt) · 全功能包 | cancelAndInterruptedDownloadRecoverWithoutAutoRestart | [x] 通过 |  |
| [UpdateDownloadTest](final-UpdateDownloadTest.txt) · 全功能包 | failedDownloadRetryDeduplicationRestoreAndTamperCheck | [x] 通过 |  |
| [UpdateDownloadTest](final-UpdateDownloadTest.txt) · 全功能包 | timeoutAndOutOfSpaceRemainRetryable | [x] 通过 |  |
| [UpdateIdleHostTest](final-UpdateIdleHostTest%23realImeDefersCachedUpdateUntilIdleAcrossRotationAndRecreation.txt) · 全功能包 | realImeDefersCachedUpdateUntilIdleAcrossRotationAndRecreation | [x] 通过 | [复测 #32](retest/32-realImeDefersCachedUpdateUntilIdleAcrossRotationAndRecreation-135331.txt) |
| [UpdateIdleHostTest](final-UpdateIdleHostTest%23seedCachedUpdate.txt) · 全功能包 | seedCachedUpdate | [x] 通过 |  |
| [UpdateInstallTest](final-UpdateInstallTest%23completeSystemUpgrade.txt) · 全功能包 | completeSystemUpgrade | [x] 通过 | [复测 #33](retest/normal-upgrade-final.json) |
| [UpdateInstallTest](final-UpdateInstallTest%23permissionDenialAndInstallerCancellationRemainRecoverable.txt) · 全功能包 | permissionDenialAndInstallerCancellationRemainRecoverable | [x] 通过 |  |
| [UpdateInstallTest](final-UpdateInstallTest%23upgradePreservedUserData.txt) · 全功能包 | upgradePreservedUserData | [x] 通过 |  |
| [UpdateIsolationTest](final-UpdateIsolationTest.txt) · 全功能包 | slowCheckAndDownloadPreserveEveryEditAndCandidateOrder | [x] 通过 |  |
| [UpdateToolbarTest](final-UpdateToolbarTest.txt) · 全功能包 | idleBadgeIsStaticAndNeverReplacesCandidatesOrDirectInput | [x] 通过 |  |

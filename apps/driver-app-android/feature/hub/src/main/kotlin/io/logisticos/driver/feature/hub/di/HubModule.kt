package io.logisticos.driver.feature.hub.di

import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent

/**
 * Hilt module for the :feature:hub module.
 *
 * [HubRepository] and [HubScanViewModel] are `@Inject`-annotated and wired
 * automatically by Hilt; this module exists as the required Hilt entry point
 * for the library module and to host any future explicit bindings.
 */
@Module
@InstallIn(SingletonComponent::class)
object HubModule
